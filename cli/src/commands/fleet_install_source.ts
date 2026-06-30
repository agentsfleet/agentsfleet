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

// A template's declared requirements — drives the install preview (mirrors the
// dashboard's install states). `trigger_present: false` means the server will
// generate a default API wake because the template shipped no TRIGGER.md.
export interface BundleRequirements {
  readonly credentials?: ReadonlyArray<string>;
  readonly tools?: ReadonlyArray<string>;
  readonly network_hosts?: ReadonlyArray<string>;
  readonly trigger_present?: boolean;
}

// A Fleet template gallery row (GET /v1/workspaces/{ws}/fleet-templates).
// `visibility` is the tier the install keys the create body off; `requirements`
// drives the preview. Metadata only — never an object-store key.
export interface FleetTemplateGalleryEntry {
  readonly id: string;
  readonly name?: string;
  readonly visibility?: string;
  readonly requirements?: BundleRequirements;
}

export interface FleetTemplateGalleryResponse {
  readonly items?: ReadonlyArray<FleetTemplateGalleryEntry>;
}

// Tier literals carried in a gallery entry's `visibility` field.
export const VISIBILITY_PLATFORM = "platform" as const;
export const VISIBILITY_TENANT = "tenant" as const;

// Fleet create body. Install keys off exactly one template tier —
// `platform_template_id` (slug) or `tenant_template_id` (UUIDv7). The live-edit
// update path (`fleet update --from`) still posts `source_markdown`/
// `trigger_markdown` to PATCH the fleet; those never ride a create. `name` is the
// optional operator override that lets one template back many fleets.
export interface CreateFleetBody {
  readonly platform_template_id?: string;
  readonly tenant_template_id?: string;
  readonly source_markdown?: string;
  readonly trigger_markdown?: string;
  readonly name?: string;
}

export const METHOD_GET = "GET" as const;
export const METHOD_POST = "POST" as const;
const TYPE_STRING = "string" as const;

// Predicate (not an inline `typeof x === TYPE_STRING`) so TypeScript narrows at
// the call site — typeof-narrowing only fires on the string literal, not a const.
export const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export const USAGE_INSTALL = "agentsfleet install --template <id>";
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

// Install requires a template id — a platform slug or a tenant UUID, resolved in
// the workspace gallery. Local-directory install was removed with the two-tier
// model; iterate on an installed fleet with `agentsfleet fleet update
// <fleet_id> --from <path>` instead.
export const requireTemplateId = (
  templateId: string | null | undefined,
): Effect.Effect<string, ValidationError> => {
  if (!isString(templateId) || templateId.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--template <id> is required",
        suggestion: `usage: ${USAGE_INSTALL}`,
      }),
    );
  }
  return Effect.succeed(templateId);
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
