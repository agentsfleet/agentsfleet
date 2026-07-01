/**
 * Fixture helpers exclusive to `install-negatives.spec.ts`.
 *
 * Two-tier install is template-only, so the negatives onboard the canonical
 * `platform-ops` sample as a tenant template (via `template-ops.ts`) and
 * exercise the install failure surface:
 *
 *   - a `--template` id absent from the workspace gallery → ConfigError, exit 5;
 *   - `install` with no `--template` → ValidationError, exit 4, no network;
 *   - the SAME onboarded template installed twice → the second trips the
 *     `(workspace_id, name)` uniqueness constraint (UZ-AGT-006, exit 3), since
 *     both installs take the template's frontmatter `name:`.
 *
 * Every onboarded name is prefixed so `cleanWorkspaceFleets` reclaims any fleet
 * this run actually managed to create.
 */

import crypto from "node:crypto";

import { ACCEPTANCE_RUN_PREFIX } from "./constants.ts";
import {
  buildPlatformOpsContent,
  onboardUploadTemplate,
  readAuthContext,
} from "./template-ops.ts";

// Exit codes the CLI maps client-side error tags to (mirrors
// `cli/src/errors/index.ts` EXIT_CODE). Repeated across assertions → named
// here per RULE UFS.
export const EXIT_CONFIG_ERROR = 5;
export const EXIT_VALIDATION_ERROR = 4;
export const EXIT_SERVER_ERROR = 3;

export const FLAG_TEMPLATE = "--template";

// Server conflict code for a duplicate fleet name within a workspace (mirrors
// `core` schema `uq_fleets_workspace_id_name` → `error_registry.zig` UZ-AGT-006,
// a 409). The CLI surfaces it as a ServerError (exit 3) carrying this code.
export const ERR_AGENTSFLEET_NAME_TAKEN = "UZ-AGT-006";

// Substring the CLI prints when `--template` resolves to no gallery entry
// (mirrors `fleet_install.ts`'s ConfigError detail).
export const ERR_TEMPLATE_NOT_IN_GALLERY = "is not in this workspace's gallery";

// A syntactically-valid UUIDv7 that is (with overwhelming probability) absent
// from the workspace gallery → the not-found path. Random tail so a stale row
// can never satisfy it.
export function absentTemplateId(): string {
  return `0195b4ba-8d3a-7f13-8abc-${crypto.randomBytes(6).toString("hex")}`;
}

export interface DuplicateTemplate {
  readonly templateId: string;
  readonly name: string;
}

/**
 * Onboard the canonical sample as a tenant template with a STABLE prefixed name.
 * Installing it twice (no `--name`) lands two fleets under the same frontmatter
 * `name:` → the second trips `uq_fleets_workspace_id_name` (UZ-AGT-006).
 */
export async function onboardDuplicateTemplate(
  env: Readonly<Record<string, string>>,
  runPrefix = ACCEPTANCE_RUN_PREFIX,
): Promise<DuplicateTemplate> {
  const name = `${runPrefix}-dup-${crypto.randomBytes(3).toString("hex")}`;
  const ctx = await readAuthContext(env);
  const content = await buildPlatformOpsContent(name);
  const templateId = await onboardUploadTemplate(ctx, content);
  return { templateId, name };
}
