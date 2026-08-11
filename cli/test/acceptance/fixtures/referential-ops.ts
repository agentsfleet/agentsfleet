/**
 * Shared wire literals and the one owned assertion for the
 * referential-integrity acceptance slice.
 *
 * The exported `SUB_*` / `FLAG_*` / `ENV_*` consts exist so no spec in the
 * suite inlines a subcommand, flag, or credential env-var name (RULE UFS).
 * The env-var names mirror `cli/src/services/config.ts` and `cli/src/cli.ts`;
 * change them there and here in the same commit.
 *
 * `assertSecretDeleteDisjunction` is the slice's real logic — see its own
 * comment for the refused-versus-cascades rule it pins.
 *
 * Auth boundary worth knowing before adding a bearer-swap test here: an
 * `agt_a…` fleet key is NOT a control-plane credential. The standard
 * `bearer()` middleware
 * (`src/agentsfleetd/auth/middleware/bearer_or_api_key.zig`) accepts only an
 * OpenID Connect (OIDC) JSON Web Token (JWT) or a tenant `agt_t` key and
 * answers 401 otherwise; `agt_a` keys are recognised exclusively on the
 * fleet-self integration-grant path
 * (`src/agentsfleetd/http/handlers/integration_grants/handler.zig`, which has
 * no read-only Command-Line Interface (CLI) command). So every control-plane
 * CLI read performed with an `agt_a` bearer is rejected at the auth boundary —
 * before AND after the key is revoked.
 */

import assert from "node:assert/strict";

import type { RunResult } from "./cli.js";
import type { ProviderSnapshot } from "./tenant-provider-ops.ts";

// --- command / flag / key wire literals (RULE UFS) -------------------------
export const SUB_CREATE = "create" as const;
export const SUB_LIST = "list" as const;
export const SUB_DELETE = "delete" as const;
export const FLAG_NAME = "--name" as const;
export const FLAG_JSON = "--json" as const;


// Auth-credential env vars (mirror the names in `cli/src/services/config.ts`
// and `cli/src/cli.ts`).
export const ENV_API_KEY = "AGENTSFLEET_API_KEY" as const;
export const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
export const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
export const ENV_NO_COLOR = "NO_COLOR" as const;
export const NO_COLOR_ON = "1" as const;

// A secret delete refused for referential reasons surfaces as a conflict
// (HTTP_409); the alternative is a clean cascade (exit 0). Dropped the bare
// `UZ-` alternative — it matched any UZ-* code, including unrelated ones.
const CONFLICT_RE = /HTTP_409|\b409\b|conflict|in[_ -]?use|referenced/i;
// `tenant provider show` flags a dangling credential reference via this marker
// (per cli/src/commands/tenant.ts).
const CREDENTIAL_MISSING = "credential_missing" as const;
// secret-delete JSON envelope key + the success status it carries.
const KEY_DELETE_STATUS = "status" as const;
const STATUS_DELETED = "deleted" as const;

/**
 * Discover-and-assert the secret-delete-under-reference disjunction so the
 * spec's `it` body stays thin (RULE: fn ≤ 50). `del` is the raw delete result;
 * `showProvider` re-reads the live posture; `providerMutated` says whether the
 * provider actually recorded a reference (only then is the posture re-checked).
 *
 *   REFUSED  : non-zero exit with a recognisable conflict; the provider still
 *              references the (still-present) secret.
 *   CASCADES : exit 0; the provider no longer hard-references a LIVE secret
 *              — the ref was dropped OR it dangles WITH a credential_missing
 *              flag. A silently-healthy posture pointing at a vanished secret is
 *              the one outcome rejected.
 */
export async function assertSecretDeleteDisjunction(opts: {
  readonly del: RunResult;
  readonly secretName: string;
  readonly providerMutated: boolean;
  readonly showProvider: () => Promise<ProviderSnapshot>;
}): Promise<void> {
  const { del, secretName, providerMutated, showProvider } = opts;
  if (del.code !== 0) {
    assert.match(`${del.stdout}\n${del.stderr}`, CONFLICT_RE,
      `refused delete had an unexpected error shape: ${del.stdout}\n${del.stderr}`);
    if (providerMutated) {
      const still = await showProvider();
      assert.equal(still.secret_ref, secretName,
        `refused delete must leave the provider reference intact: ${JSON.stringify(still)}`);
    }
    return;
  }
  const status = (JSON.parse(del.stdout.trim() || "{}") as Record<string, unknown>)[KEY_DELETE_STATUS];
  assert.equal(status, STATUS_DELETED, `unexpected secret delete status: ${del.stdout}`);
  if (!providerMutated) return;
  const after = await showProvider();
  const danglingButFlagged = after.secret_ref === secretName && after.error === CREDENTIAL_MISSING;
  const refDropped = after.secret_ref !== secretName;
  assert.ok(danglingButFlagged || refDropped,
    `cascading secret delete left an unflagged dangling provider reference: ${JSON.stringify(after)}`);
}
