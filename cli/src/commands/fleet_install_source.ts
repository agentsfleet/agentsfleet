// Install source resolution + body shaping — split out of fleet_install.ts to
// keep that file under the 350-line FLL cap. The install/update Effects compose
// these helpers; nothing here performs IO beyond the requirements print, so the
// source-selection rules (exactly one of --from / --template) and the
// create-body shaping stay unit-testable in isolation.

import { Effect } from "effect";
import { Output } from "../services/output.ts";
import { ValidationError } from "../errors/index.ts";
import type { LoadedSkill } from "../lib/load-skill-from-path.ts";

export interface InstallResponse {
  readonly fleet_id?: string;
  readonly name?: string;
  readonly webhook_urls?: Record<string, string>;
}

export interface UpdateResponse {
  readonly config_revision?: number | string | null;
}

// Parsed requirements of an imported bundle — drives the install preview
// (mirrors the dashboard's install states). `trigger_present: false` means the
// server generated a default API wake because the bundle shipped no TRIGGER.md.
export interface BundleRequirements {
  readonly credentials?: ReadonlyArray<string>;
  readonly tools?: ReadonlyArray<string>;
  readonly network_hosts?: ReadonlyArray<string>;
  readonly trigger_present?: boolean;
}

// POST /v1/workspaces/{ws}/fleets/bundles/snapshots response — the content-
// addressed snapshot the create call then references by `bundle_id`.
export interface BundleSnapshot {
  readonly bundle_id?: string;
  readonly name?: string;
  readonly requirements?: BundleRequirements;
}

// Fleet create body — `bundle_id` (template/import path) and direct
// `source_markdown`/`trigger_markdown` are mutually exclusive sources; `name`
// is the optional operator override that lets one bundle back many fleets.
export interface CreateFleetBody {
  readonly source_markdown?: string;
  readonly trigger_markdown?: string;
  readonly bundle_id?: string;
  readonly name?: string;
}

// Install sources are mutually exclusive — exactly one of a local bundle path
// or a first-party template id. Tagged union so the resolved source carries
// only the field its kind needs.
export type InstallSource =
  | { readonly kind: typeof SOURCE_PATH; readonly fromPath: string }
  | { readonly kind: typeof SOURCE_TEMPLATE; readonly templateId: string };

export const SOURCE_PATH = "path" as const;
// Internal InstallSource discriminant. Its value coincides with
// SOURCE_KIND_TEMPLATE below but the two answer to different owners — this one
// is a local tag, that one is the wire value — so keep them separate.
export const SOURCE_TEMPLATE = "template" as const;
// Wire value for ImportBundleRequest.source_kind (the only kind the CLI
// imports; `upload`/`github` are dashboard-only today).
export const SOURCE_KIND_TEMPLATE = "template" as const;
export const METHOD_POST = "POST" as const;
const TYPE_STRING = "string" as const;

// Predicate (not an inline `typeof x === TYPE_STRING`) so TypeScript narrows at
// the call site — typeof-narrowing only fires on the string literal, not a const.
export const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export const USAGE_INSTALL = "agentsfleet install (--from <path> | --template <id>)";
export const USAGE_UPDATE = "agentsfleet fleet update <fleet_id> --from <path>";

export const bodyFromBundle = (
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

export const requireFromPath = (
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
export const resolveSource = (
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
export const withName = (
  body: CreateFleetBody,
  name: string | null | undefined,
): CreateFleetBody => {
  const trimmed = isString(name) ? name.trim() : "";
  return trimmed.length > 0 ? { ...body, name: trimmed } : body;
};

// Install preview — the credential/tool/host requirements the operator must
// wire before the Fleet can run. Mirrors the dashboard's connect-to-continue.
export const printRequirements = (
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
