// Fleet install + update — Effects mirroring the imperative leaves
// they replaced. Each reads the workspace-scoped MainLayer services
// (CliConfig + Output + HttpClient + Credentials + Workspaces) and
// emits CliError variants on failure.
//
// `loadSkillFromPath` is sync filesystem IO — wrapped in `Effect.try`
// so the SkillLoadError surfaces on the typed error channel as a
// ConfigError (operator can act on the message).

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetsPath, wsFleetPath } from "../lib/api-paths.ts";
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

interface InstallResponse {
  readonly fleet_id?: string;
  readonly name?: string;
  readonly webhook_urls?: Record<string, string>;
}

interface UpdateResponse {
  readonly config_revision?: number | string | null;
}

const USAGE_INSTALL = "agentsfleet install --from <path>";
const USAGE_UPDATE =
  "agentsfleet fleet update <fleet_id> --from <path>";

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

const bodyFromBundle = (
  bundle: LoadedSkill,
): { source_markdown: string; trigger_markdown?: string } => {
  if (bundle.trigger_md === null) {
    return { source_markdown: bundle.skill_md };
  }
  return {
    source_markdown: bundle.skill_md,
    trigger_markdown: bundle.trigger_md,
  };
};

const requireFromPath = (
  fromPath: string | null | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> => {
  if (typeof fromPath !== "string" || fromPath.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--from <path> is required",
        suggestion: `usage: ${usage}`,
      }),
    );
  }
  return Effect.succeed(fromPath);
};

export const installEffectFromFlags = (
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

    const path = yield* requireFromPath(fromPath, USAGE_INSTALL);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<InstallResponse>({
      path: wsFleetsPath(wsId),
      method: "POST",
      body: bodyFromBundle(bundle),
      token,
    });

    const displayName = res.name || bundle.fallback_name;
    const generatedTrigger = bundle.trigger_md === null;

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
