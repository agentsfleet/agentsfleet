// Codified version of the install-skill's step-8 placeholder substitution.
// The agent following SKILL.md does this textually before writing the
// generated `.usezombie/platform-ops/{SKILL,TRIGGER}.md` files. Tests use
// this helper to assert the substitution is total and correct.

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const repoRoot = resolve(__dirname, "..", "..", "..");
export const platformOpsDir = resolve(repoRoot, "samples", "platform-ops");

/** Apply `{{key}}` placeholder substitution. Throws on any `{{...}}` left
 * after substitution — the install-skill must produce a fully-substituted
 * file or stop. Empty values are valid (`""` for BYOK model, `0` for
 * BYOK cap), but the placeholder itself must always be replaced. */
export function substitute(template, vars) {
  let out = template;
  for (const [k, v] of Object.entries(vars)) {
    out = out.split(`{{${k}}}`).join(String(v));
  }
  const leftover = out.match(/\{\{[a-z_]+\}\}/);
  if (leftover) {
    throw new Error(`unsubstituted placeholder: ${leftover[0]}`);
  }
  return out;
}

export function readTemplate(name) {
  return readFileSync(resolve(platformOpsDir, name), "utf8");
}

/** The five canonical placeholders the install-skill substitutes. */
export const PLACEHOLDERS = [
  "slack_channel",
  "prod_branch_glob",
  "cron_schedule",
  "model",
  "context_cap_tokens",
];

/** The doctor `tenant_provider` block under platform-managed posture
 * (resolved values pinned into frontmatter). */
export const PLATFORM_DEFAULTS = {
  model: "accounts/fireworks/models/kimi-k2.6",
  context_cap_tokens: 256000,
};

/** The visible BYOK sentinels (worker overlays from tenant_providers). */
export const BYOK_SENTINELS = {
  model: "",
  context_cap_tokens: 0,
};
