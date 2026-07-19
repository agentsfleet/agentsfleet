/**
 * Owned helpers for the referential-integrity acceptance slice.
 *
 * Nothing else in the suite mints a Fleet key and then tries to re-authenticate
 * a CLI read WITH that key, so this file carries the primitives that round-trip
 * is built from:
 *
 *   - `mintFleetKey`  — `fleet-key create --fleet <id> --name <prefixed> --json`,
 *     returns BOTH the `fleet_key_id` (for revoke/teardown) and the usable
 *     `key` secret (the `agt_a…` value the create response exposes once). The
 *     secret field is confirmed against `cli/src/commands/fleet_key.ts`
 *     (`FleetKeyResponse.key`, emitted verbatim by `output.printJson(res)`).
 *
 *   - `readWithFleetKey` — runs one CLI read with the fleet key injected as the
 *     bearer. The Effect command path resolves its bearer from
 *     `config.accessToken` ← `AGENTSFLEET_API_KEY` (env slot; see
 *     `cli/src/services/config.ts` + `workspace-guards.ts#resolveAuthToken`),
 *     which wins over any stored login JWT via `resolveToken`'s env-first
 *     precedence. So the fleet key is injected as `AGENTSFLEET_API_KEY` — the
 *     env var that carries a bearer to the wire for these commands. This helper
 *     makes no claim about the OUTCOME; it returns the raw run result and the
 *     caller asserts.
 *
 *     Rule verified against `agentsfleetd`: an `agt_a…` fleet key is NOT a
 *     control-plane credential. The standard `bearer()` middleware
 *     (`src/agentsfleetd/auth/middleware/bearer_or_api_key.zig`) accepts only an
 *     OIDC JWT or a tenant `agt_t` key and answers 401 otherwise; `agt_a` keys
 *     are recognised exclusively on the fleet-self integration-grant path
 *     (`src/agentsfleetd/http/handlers/integration_grants/handler.zig`, which has
 *     no read-only CLI command). So every control-plane CLI read performed with
 *     an `agt_a` bearer is REJECTED at the auth boundary — before AND after the
 *     key is revoked. The spec pins that real boundary rather than presuming the
 *     key authenticates a control-plane read.
 *
 *   - `revokeFleetKey` — `fleet-key delete <id> --json`, best-effort for
 *     teardown and the load-bearing revoke step.
 *
 * The read uses the SAME hydrated state dir as the JWT identity (so a workspace
 * context already exists on disk); only the bearer token is swapped. No global
 * emptiness is ever asserted; every name is `ACCEPTANCE_RUN_PREFIX`-scoped by
 * the caller and revoked/cleaned in `afterAll`.
 */

import assert from "node:assert/strict";

import { ERR_UNAUTHORIZED } from "../../../src/errors/auth.ts";
import { composeEnv, runFleetctl } from "./cli.js";
import type { RunResult } from "./cli.js";
import { assertNoSecretLeak } from "./negatives.ts";
import type { ProviderSnapshot } from "./tenant-provider-ops.ts";

type Env = Readonly<Record<string, string>>;

// --- command / flag / key wire literals (RULE UFS) -------------------------
export const CMD_AGENT_KEY = "fleet-key" as const;
export const SUB_CREATE = "create" as const;
export const SUB_LIST = "list" as const;
export const SUB_DELETE = "delete" as const;
export const FLAG_AGENT = "--fleet" as const;
export const FLAG_NAME = "--name" as const;
export const FLAG_JSON = "--json" as const;

export const KEY_AGENT_KEY_ID = "fleet_key_id" as const;
export const KEY_SECRET = "key" as const;

// Auth-credential env vars (mirror the names in `cli/src/services/config.ts`
// and `cli/src/cli.ts`). The fleet key is injected as the API-key var — the
// env slot that carries the bearer to the wire (winning over a stored login).
export const ENV_API_KEY = "AGENTSFLEET_API_KEY" as const;
export const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
export const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
export const ENV_NO_COLOR = "NO_COLOR" as const;
export const NO_COLOR_ON = "1" as const;

// The `agt_a…` prefix the runner issues to external fleet keys (per the
// header comment in `cli/src/commands/fleet_key.ts` and the single-source pin
// in `src/agentsfleetd/auth/api_key.zig`). Used only as a shape sanity-check
// on the minted secret, never asserted to be exact.
export const AGENT_KEY_SECRET_PREFIX = "agt_a" as const;

// An `agt_a` fleet key sent as a control-plane bearer is REJECTED at the auth
// boundary — the `bearer()` middleware answers 401 (no JWT / no `agt_t`), which
// the CLI surfaces primarily as UZ-AUTH-002, with HTTP_401 / HTTP_403 stems or
// wording tolerated as fallbacks. The same rejection holds after the key is
// revoked. Anchored to auth-rejection forms only — a bare `invalid`/`expired`
// would also match unrelated errors
// ("invalid argument", "expired snapshot") and let a wrong-reason failure
// pass as an auth rejection.
const REJECTED_AUTH_PATTERN =
  `${ERR_UNAUTHORIZED}|HTTP_401|HTTP_403|\\b401\\b|\\b403\\b|unauthor|forbidden`;
export const REJECTED_AUTH_RE = new RegExp(REJECTED_AUTH_PATTERN, "i");

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

export interface MintedFleetKey {
  /** Stable id used to revoke the key and for teardown. */
  readonly fleetKeyId: string;
  /** The usable secret (`agt_a…`) shown once by the create response. */
  readonly secret: string;
}

interface FleetKeyAddEnvelope {
  readonly fleet_key_id?: unknown;
  readonly key?: unknown;
}

/**
 * `fleet-key create --fleet <id> --name <name> --json` → both ids. Asserts a
 * clean exit and that the create response exposed a usable secret. Throws (rather
 * than returning a partial) so the caller never authenticates with `undefined`.
 */
export async function mintFleetKey(
  env: Env,
  sessionJwt: string,
  opts: { readonly fleetId: string; readonly name: string },
): Promise<MintedFleetKey> {
  const result = await runFleetctl(
    [CMD_AGENT_KEY, SUB_CREATE, FLAG_AGENT, opts.fleetId, FLAG_NAME, opts.name, FLAG_JSON],
    { env, stdin: "" },
  );
  assertNoSecretLeak(result, sessionJwt);
  assert.equal(result.code, 0, `fleet-key create exited ${result.code}: ${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim()) as FleetKeyAddEnvelope;
  const fleetKeyId = parsed[KEY_AGENT_KEY_ID];
  const secret = parsed[KEY_SECRET];
  assert.equal(typeof fleetKeyId, "string", `create missing ${KEY_AGENT_KEY_ID}: ${result.stdout}`);
  assert.equal(typeof secret, "string", `create missing usable ${KEY_SECRET} secret: ${result.stdout}`);
  assert.ok((secret as string).length > 0, `create returned an empty ${KEY_SECRET} secret: ${result.stdout}`);
  return { fleetKeyId: fleetKeyId as string, secret: secret as string };
}

/**
 * Perform one CLI read with the fleet key as the bearer. Reuses the JWT
 * identity's hydrated state dir (so the workspace context already exists on
 * disk) and swaps ONLY the token. Returns the raw run result; the caller
 * asserts the outcome (an `agt_a` key is rejected on control-plane reads —
 * see this file's header — so the caller asserts rejection, not success).
 */
export async function readWithFleetKey(
  baseEnv: Env,
  fleetKeySecret: string,
  args: ReadonlyArray<string>,
): Promise<RunResult> {
  const keyEnv = composeEnv({
    [ENV_API_KEY]: fleetKeySecret,
    [ENV_API_URL]: baseEnv[ENV_API_URL],
    [ENV_STATE_DIR]: baseEnv[ENV_STATE_DIR],
    [ENV_NO_COLOR]: NO_COLOR_ON,
  });
  const result = await runFleetctl([...args, FLAG_JSON], { env: keyEnv, stdin: "" });
  // The bearer here IS the fleet key, not the JWT — but the key must never
  // echo into stderr/stdout in plaintext.
  assertNoSecretLeak(result, fleetKeySecret);
  return result;
}

/** `fleet-key delete <id> --json` — best-effort revoke (teardown + the test). */
export async function revokeFleetKey(env: Env, fleetKeyId: string): Promise<RunResult> {
  return runFleetctl([CMD_AGENT_KEY, SUB_DELETE, fleetKeyId, FLAG_JSON], { env, stdin: "" });
}
