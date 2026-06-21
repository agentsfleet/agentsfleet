// Fleet install + update — Effects mirroring the imperative leaves
// they replaced. Each reads the workspace-scoped MainLayer services
// (CliConfig + Output + HttpClient + Credentials + Workspaces) and
// emits CliError variants on failure.
//
// `loadSkillFromPath` is sync filesystem IO — wrapped in `Effect.try`
// so the SkillLoadError surfaces on the typed error channel as a
// ConfigError (operator can act on the message).

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
  wsFleetBundleSnapshotsPath,
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

interface InstallResponse {
  readonly fleet_id?: string;
  readonly name?: string;
  readonly webhook_urls?: Record<string, string>;
}

interface UpdateResponse {
  readonly config_revision?: number | string | null;
}

// Parsed requirements of an imported bundle — drives the install preview
// (mirrors the dashboard's BundlePreview). `trigger_present: false` means the
// server generated a default API wake because the bundle shipped no TRIGGER.md.
interface BundleRequirements {
  readonly credentials?: ReadonlyArray<string>;
  readonly tools?: ReadonlyArray<string>;
  readonly network_hosts?: ReadonlyArray<string>;
  readonly trigger_present?: boolean;
}

// POST /v1/workspaces/{ws}/fleets/bundles/snapshots response — the content-
// addressed snapshot the create call then references by `bundle_id`.
interface BundleSnapshot {
  readonly bundle_id?: string;
  readonly name?: string;
  readonly requirements?: BundleRequirements;
}

// Fleet create body — `bundle_id` (template/import path) and direct
// `source_markdown`/`trigger_markdown` are mutually exclusive sources; `name`
// is the optional operator override that lets one bundle back many fleets.
interface CreateFleetBody {
  readonly source_markdown?: string;
  readonly trigger_markdown?: string;
  readonly bundle_id?: string;
  readonly name?: string;
}

// Install sources are mutually exclusive — exactly one of a local bundle path
// or a first-party template id. Tagged union so the resolved source carries
// only the field its kind needs.
type InstallSource =
  | { readonly kind: typeof SOURCE_PATH; readonly fromPath: string }
  | { readonly kind: typeof SOURCE_TEMPLATE; readonly templateId: string };

export interface InstallFlags {
  readonly fromPath?: string | null | undefined;
  readonly templateId?: string | null | undefined;
  readonly name?: string | null | undefined;
}

const SOURCE_PATH = "path" as const;
// Internal InstallSource discriminant. Its value coincides with
// SOURCE_KIND_TEMPLATE below but the two answer to different owners — this one
// is a local tag, that one is the wire value — so keep them separate.
const SOURCE_TEMPLATE = "template" as const;
// Wire value for ImportBundleRequest.source_kind (the only kind the CLI
// imports; `upload`/`github` are dashboard-only today).
const SOURCE_KIND_TEMPLATE = "template" as const;
const TYPE_STRING = "string" as const;
const METHOD_POST = "POST" as const;

// Predicate (not an inline `typeof x === TYPE_STRING`) so TypeScript narrows at
// the call site — typeof-narrowing only fires on the string literal, not a const.
const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

const USAGE_INSTALL = "agentsfleet install (--from <path> | --template <id>)";
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
  if (!isString(fromPath) || fromPath.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--from <path> is required",
        suggestion: `usage: ${usage}`,
      }),
    );
  }
  return Effect.succeed(fromPath);
};

// Exactly one source: a local bundle path (`--from`) or a template id
// (`--template`). Neither is the common first-run mistake; both is an
// ambiguous request — both fail with the usage line rather than silently
// preferring one.
const resolveSource = (
  fromPath: string | null | undefined,
  templateId: string | null | undefined,
): Effect.Effect<InstallSource, ValidationError> => {
  const hasPath = isString(fromPath) && fromPath.length > 0;
  const hasTemplate = isString(templateId) && templateId.length > 0;
  if (hasPath && hasTemplate) {
    return Effect.fail(
      new ValidationError({
        detail: "--from and --template are mutually exclusive",
        suggestion: `usage: ${USAGE_INSTALL}`,
      }),
    );
  }
  if (hasPath) return Effect.succeed({ kind: SOURCE_PATH, fromPath });
  if (hasTemplate) return Effect.succeed({ kind: SOURCE_TEMPLATE, templateId });
  return Effect.fail(
    new ValidationError({
      detail: "a source is required: --from <path> or --template <id>",
      suggestion: `usage: ${USAGE_INSTALL}`,
    }),
  );
};

// Fold an optional operator name override into a create body. A blank/whitespace
// name is treated as absent so the server falls back to the SKILL.md `name:`.
const withName = (
  body: CreateFleetBody,
  name: string | null | undefined,
): CreateFleetBody => {
  const trimmed = isString(name) ? name.trim() : "";
  return trimmed.length > 0 ? { ...body, name: trimmed } : body;
};

// Install preview — the credential/tool/host requirements the operator must
// wire before the Fleet can run. Mirrors the dashboard's BundlePreview.
const printRequirements = (
  req: BundleRequirements | undefined,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    if (!req) return;
    const output = yield* Output;
    const rows: ReadonlyArray<readonly [string, ReadonlyArray<string> | undefined]> = [
      ["Credentials", req.credentials],
      ["Tools", req.tools],
      ["Network hosts", req.network_hosts],
    ];
    for (const [label, values] of rows) {
      if (values && values.length > 0) {
        yield* output.info(`  ${label}: ${values.join(", ")}`);
      }
    }
  });

// POST the create + render the install result. Shared by both sources so the
// success / JSON output stays identical whether the bundle came from a path or
// a template snapshot.
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
    const http = yield* HttpClient;

    const source = yield* resolveSource(flags.fromPath, flags.templateId);
    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    if (source.kind === SOURCE_TEMPLATE) {
      const snapshot = yield* http.request<BundleSnapshot>({
        path: wsFleetBundleSnapshotsPath(wsId),
        method: METHOD_POST,
        body: { source_kind: SOURCE_KIND_TEMPLATE, source_ref: source.templateId },
        token,
      });
      yield* printRequirements(snapshot.requirements);
      const bundleId = snapshot.bundle_id;
      if (!bundleId) {
        return yield* Effect.fail(
          new ConfigError({
            detail: "import did not return a bundle_id",
            suggestion: "retry, or report if it persists",
          }),
        );
      }
      const body = withName({ bundle_id: bundleId }, flags.name);
      const generatedTrigger = snapshot.requirements?.trigger_present === false;
      const fallbackName = snapshot.name || source.templateId;
      yield* createAndRender(wsId, token, body, generatedTrigger, fallbackName);
      return;
    }

    const bundle = yield* loadBundle(source.fromPath);
    const body = withName(bodyFromBundle(bundle), flags.name);
    const generatedTrigger = bundle.trigger_md === null;
    yield* createAndRender(wsId, token, body, generatedTrigger, bundle.fallback_name);
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
