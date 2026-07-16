// Fleet install + update — Effects mirroring the imperative leaves
// they replaced. Each reads the workspace-scoped MainLayer services
// (CliConfig + Output + HttpClient + Credentials + Workspaces) and
// emits CliError variants on failure.
//
// `loadSkillFromPath` is sync filesystem IO — wrapped in `Effect.try`
// so the SkillLoadError surfaces on the typed error channel as a
// ConfigError (operator can act on the message).
//
// Source resolution + body shaping live in fleet_install_source.ts (split out
// to keep this file under the FLL cap); this file owns the create/update
// orchestration + result rendering.

import { Effect, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import {
  wsFleetsPath,
  wsFleetPath,
  wsFleetLibrariesPath,
} from "../lib/api-paths.ts";
import {
  loadSkillFromPath,
  SkillLoadError,
  type LoadedSkill,
} from "../lib/load-skill-from-path.ts";
import { validateRequiredId } from "../program/validators.ts";
import { OPT_FROM } from "../constants/cli-flags.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";
import {
  bodyFromBundle,
  METHOD_GET,
  METHOD_POST,
  printRequirements,
  requireFromPath,
  requireLibraryId,
  USAGE_UPDATE,
  VISIBILITY_PLATFORM,
  VISIBILITY_TENANT,
  withName,
  type CreateFleetBody,
  type FleetLibraryGalleryResponse,
  type InstallResponse,
  type UpdateResponse,
} from "./fleet_install_source.ts";

export interface InstallFlags {
  readonly libraryId?: string | null | undefined;
  readonly name?: string | null | undefined;
}

// `loader` is injectable (defaults to the real filesystem load) so the
// non-SkillLoadError catch arms are reachable in a unit test. The ladder is
// defensive: loadSkillFromPath only throws SkillLoadError today, but a future
// foreign throw (TypeError, OutOfMemory, a bare string) must still render a
// readable detail instead of `undefined: ...`.
export const loadBundle = (
  fromPath: string,
  loader: (path: string) => LoadedSkill = loadSkillFromPath,
): Effect.Effect<LoadedSkill, ConfigError> =>
  Effect.try({
    try: () => loader(fromPath),
    catch: (err) =>
      new ConfigError({
        // SkillLoadError carries a typed code; any other throw falls back to
        // its message (Error) or its string form (non-Error) — never the
        // `undefined: ...` a blind `err as SkillLoadError` cast would print.
        detail:
          err instanceof SkillLoadError
            ? `${err.code}: ${err.message}`
            : String((err as Error)?.message ?? err),
        suggestion: "verify the path exists and contains SKILL.md; TRIGGER.md is optional",
      }),
  });

// POST the create + render the install result. Shared by both sources so the
// success / JSON output stays identical whether the bundle came from a path or
// a library entry snapshot.
const createAndRender = (
  wsId: string,
  token: Redacted.Redacted<string>,
  body: CreateFleetBody,
  generatedTrigger: boolean,
  fallbackName: string,
): Effect.Effect<void, CliError, CliConfig | HttpClient | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const res = yield* http.request<InstallResponse>({
      path: wsFleetsPath(wsId),
      method: METHOD_POST,
      body,
      token,
    });

    const displayName = res.name || fallbackName;

    if (config.jsonMode) {
      yield* output.printJson({
        status: "installed",
        fleet_id: res.fleet_id,
        webhook_urls: res.webhook_urls ?? {},
        name: displayName,
        generated_trigger: generatedTrigger,
      });
      return;
    }

    yield* output.success(`${displayName} is live.`);
    if (generatedTrigger) {
      yield* output.info("  Generated default API wake because TRIGGER.md was not present.");
    }
    if (res.fleet_id) yield* output.info(`  Fleet ID: ${res.fleet_id}`);
    const urls = res.webhook_urls ?? {};
    const sources = Object.keys(urls);
    if (sources.length > 0) {
      yield* output.info("  Webhook URLs (register on the upstream provider):");
      for (const source of sources) {
        yield* output.info(`    ${source}: ${urls[source]}`);
      }
    }
  });

export const installEffectFromFlags = (
  flags: InstallFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const http = yield* HttpClient;

    const libraryId = yield* requireLibraryId(flags.libraryId);
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    // Resolve the library entry in the workspace gallery (platform ∪ tenant). The
    // entry carries its tier + declared requirements; the create body keys off
    // `visibility` (M103 §4). No snapshot import — the server reads SKILL/TRIGGER
    // from the onboarded library entry row.
    const gallery = yield* http.request<FleetLibraryGalleryResponse>({
      path: wsFleetLibrariesPath(wsId),
      method: METHOD_GET,
      token,
    });
    const entry = (gallery.items ?? []).find((e) => e.id === libraryId);
    if (!entry) {
      return yield* Effect.fail(
        new ConfigError({
          detail: `library entry '${libraryId}' is not in this workspace's gallery`,
          suggestion: "run `agentsfleet library` for platform ids; tenant library ids come from your workspace gallery in the dashboard",
        }),
      );
    }
    if (!config.jsonMode) yield* printRequirements(entry.requirements);
    // Key the create body off the resolved tier. Fail loud on an unrecognized
    // visibility rather than silently posting a platform slug as a tenant id.
    if (entry.visibility !== VISIBILITY_PLATFORM && entry.visibility !== VISIBILITY_TENANT) {
      return yield* Effect.fail(
        new ConfigError({
          detail: `library entry '${libraryId}' has an unrecognized tier '${entry.visibility ?? ""}'`,
          suggestion: "update the CLI to a version that understands this library tier",
        }),
      );
    }
    const idBody: CreateFleetBody =
      entry.visibility === VISIBILITY_PLATFORM
        ? { platform_library_id: entry.id }
        : { tenant_library_id: entry.id };
    const body = withName(idBody, flags.name);
    const generatedTrigger = entry.requirements?.trigger_present === false;
    const fallbackName = entry.name || libraryId;
    yield* createAndRender(wsId, token, body, generatedTrigger, fallbackName);
  });

export const updateEffectFromArgs = (
  fleetId: string | undefined,
  fromPath: string | null | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    if (!fleetId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "fleet_id is required",
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }
    const idCheck = validateRequiredId(fleetId, "fleet_id");
    if (!idCheck.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: idCheck.message,
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }

    const path = yield* requireFromPath(fromPath, USAGE_UPDATE);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<UpdateResponse>({
      path: wsFleetPath(wsId, fleetId),
      method: "PATCH",
      body: bodyFromBundle(bundle),
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({
        status: "updated",
        fleet_id: fleetId,
        config_revision: res.config_revision,
      });
      return;
    }

    yield* output.success(`${fleetId} updated.`);
    if (res.config_revision != null) {
      yield* output.info(`  Config revision: ${res.config_revision}`);
    }
  });

export { OPT_FROM };
