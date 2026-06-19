/**
 * Owned helpers for the referential-integrity acceptance slice.
 *
 * Nothing else in the suite mints an agent key and then tries to re-authenticate
 * a CLI read WITH that key, so this file carries the primitives that round-trip
 * is built from:
 *
 *   - `mintAgentKey`  — `agent-key add --agent <id> --name <prefixed> --json`,
 *     returns BOTH the `agent_key_id` (for revoke/teardown) and the usable
 *     `key` secret (the `agt_a…` value the add response exposes once). The
 *     secret field is confirmed against `cli/src/commands/agent_key.ts`
 *     (`AgentKeyResponse.key`, emitted verbatim by `output.printJson(res)`).
 *
 *   - `readWithAgentKey` — runs one CLI read with the agent key injected as the
 *     bearer. The Effect command path resolves its bearer from
 *     `config.accessToken` ← `AGENTSFLEET_TOKEN` (see `cli/src/services/config.ts`
 *     + `workspace-guards.ts#resolveAuthToken`); `AGENTSFLEET_API_KEY` feeds only
 *     the legacy `CommandCtx` path the Effect-shaped read commands never consult.
 *     So the agent key is injected as `AGENTSFLEET_TOKEN` — the var that actually
 *     carries a bearer to the wire for these commands. This helper makes no claim
 *     about the OUTCOME; it returns the raw run result and the caller asserts.
 *
 *     CONTRACT (verified against `agentsfleetd`): an `agt_a…` agent key is NOT a
 *     control-plane credential. The standard `bearer()` middleware
 *     (`src/agentsfleetd/auth/middleware/bearer_or_api_key.zig`) accepts only an
 *     OIDC JWT or a tenant `agt_t` key and answers 401 otherwise; `agt_a` keys
 *     are recognised exclusively on the agent-self integration-grant path
 *     (`src/agentsfleetd/http/handlers/integration_grants/handler.zig`, which has
 *     no read-only CLI command). So every control-plane CLI read performed with
 *     an `agt_a` bearer is REJECTED at the auth boundary — before AND after the
 *     key is revoked. The spec pins that real boundary rather than presuming the
 *     key authenticates a control-plane read.
 *
 *   - `revokeAgentKey` — `agent-key delete <id> --json`, best-effort for
 *     teardown and the load-bearing revoke step.
 *
 * The read uses the SAME hydrated state dir as the JWT identity (so a workspace
 * context already exists on disk); only the bearer token is swapped. No global
 * emptiness is ever asserted; every name is `ACCEPTANCE_RUN_PREFIX`-scoped by
 * the caller and revoked/cleaned in `afterAll`.
 */

import assert from "node:assert/strict";

import { composeEnv, runAgentctl } from "./cli.js";
import type { RunResult } from "./cli.js";
import { assertNoSecretLeak } from "./negatives.ts";
import type { ProviderSnapshot } from "./tenant-provider-ops.ts";

type Env = Readonly<Record<string, string>>;

// --- command / flag / key wire literals (RULE UFS) -------------------------
export const CMD_AGENT_KEY = "agent-key" as const;
export const SUB_ADD = "add" as const;
export const SUB_LIST = "list" as const;
export const SUB_DELETE = "delete" as const;
export const FLAG_AGENT = "--agent" as const;
export const FLAG_NAME = "--name" as const;
export const FLAG_JSON = "--json" as const;

export const KEY_AGENT_KEY_ID = "agent_key_id" as const;
export const KEY_SECRET = "key" as const;

// Auth-credential env vars (mirror the names in `cli/src/services/config.ts`
// and `cli/src/cli.ts`). The agent key is injected as the TOKEN var — that is
// the bearer the Effect command path resolves; API_KEY only feeds the legacy
// CommandCtx path the read commands ignore.
export const ENV_TOKEN = "AGENTSFLEET_TOKEN" as const;
export const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
export const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
export const ENV_NO_COLOR = "NO_COLOR" as const;
export const NO_COLOR_ON = "1" as const;

// The `agt_a…` prefix the runner issues to external agent keys (per the
// header comment in `cli/src/commands/agent_key.ts` and the single-source pin
// in `src/agentsfleetd/auth/api_key.zig`). Used only as a shape sanity-check
// on the minted secret, never asserted to be exact.
export const AGENT_KEY_SECRET_PREFIX = "agt_a" as const;

// An `agt_a` agent key sent as a control-plane bearer is REJECTED at the auth
// boundary — the `bearer()` middleware answers 401 (no JWT / no `agt_t`), which
// the CLI surfaces as an `HTTP_401` / `HTTP_403` ServerError stem. The same
// rejection holds after the key is revoked. This regex matches either stem.
export const REJECTED_AUTH_RE = /HTTP_401|HTTP_403|401|403|unauthor|forbidden|invalid|expired/i;

// A credential delete refused for referential reasons surfaces as a conflict
// (HTTP_409 / an in-use UZ-* code); the alternative is a clean cascade (exit 0).
const CONFLICT_RE = /HTTP_409|409|conflict|in[_ -]?use|referenced|UZ-/i;
// `tenant provider show` flags a dangling credential reference via this marker
// (per cli/src/commands/tenant.ts).
const CREDENTIAL_MISSING = "credential_missing" as const;
// credential-delete JSON envelope key + the success status it carries.
const KEY_DELETE_STATUS = "status" as const;
const STATUS_DELETED = "deleted" as const;

/**
 * Discover-and-assert the credential-delete-under-reference disjunction so the
 * spec's `it` body stays thin (RULE: fn ≤ 50). `del` is the raw delete result;
 * `showProvider` re-reads the live posture; `providerMutated` says whether the
 * provider actually recorded a reference (only then is the posture re-checked).
 *
 *   REFUSED  : non-zero exit with a recognisable conflict; the provider still
 *              references the (still-present) credential.
 *   CASCADES : exit 0; the provider no longer hard-references a LIVE credential
 *              — the ref was dropped OR it dangles WITH a credential_missing
 *              flag. A silently-healthy posture pointing at a vanished secret is
 *              the one outcome rejected.
 */
export async function assertCredentialDeleteDisjunction(opts: {
  readonly del: RunResult;
  readonly credName: string;
  readonly providerMutated: boolean;
  readonly showProvider: () => Promise<ProviderSnapshot>;
}): Promise<void> {
  const { del, credName, providerMutated, showProvider } = opts;
  if (del.code !== 0) {
    assert.match(`${del.stdout}\n${del.stderr}`, CONFLICT_RE,
      `refused delete had an unexpected error shape: ${del.stdout}\n${del.stderr}`);
    if (providerMutated) {
      const still = await showProvider();
      assert.equal(still.credential_ref, credName,
        `refused delete must leave the provider reference intact: ${JSON.stringify(still)}`);
    }
    return;
  }
  const status = (JSON.parse(del.stdout.trim() || "{}") as Record<string, unknown>)[KEY_DELETE_STATUS];
  assert.equal(status, STATUS_DELETED, `unexpected credential delete status: ${del.stdout}`);
  if (!providerMutated) return;
  const after = await showProvider();
  const danglingButFlagged = after.credential_ref === credName && after.error === CREDENTIAL_MISSING;
  const refDropped = after.credential_ref !== credName;
  assert.ok(danglingButFlagged || refDropped,
    `cascading credential delete left an unflagged dangling provider reference: ${JSON.stringify(after)}`);
}

export interface MintedAgentKey {
  /** Stable id used to revoke the key and for teardown. */
  readonly agentKeyId: string;
  /** The usable secret (`agt_a…`) shown once by the add response. */
  readonly secret: string;
}

interface AgentKeyAddEnvelope {
  readonly agent_key_id?: unknown;
  readonly key?: unknown;
}

/**
 * `agent-key add --agent <id> --name <name> --json` → both ids. Asserts a
 * clean exit and that the add response exposed a usable secret. Throws (rather
 * than returning a partial) so the caller never authenticates with `undefined`.
 */
export async function mintAgentKey(
  env: Env,
  sessionJwt: string,
  opts: { readonly agentId: string; readonly name: string },
): Promise<MintedAgentKey> {
  const result = await runAgentctl(
    [CMD_AGENT_KEY, SUB_ADD, FLAG_AGENT, opts.agentId, FLAG_NAME, opts.name, FLAG_JSON],
    { env, stdin: "" },
  );
  assertNoSecretLeak(result, sessionJwt);
  assert.equal(result.code, 0, `agent-key add exited ${result.code}: ${result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim()) as AgentKeyAddEnvelope;
  const agentKeyId = parsed[KEY_AGENT_KEY_ID];
  const secret = parsed[KEY_SECRET];
  assert.equal(typeof agentKeyId, "string", `add missing ${KEY_AGENT_KEY_ID}: ${result.stdout}`);
  assert.equal(typeof secret, "string", `add missing usable ${KEY_SECRET} secret: ${result.stdout}`);
  assert.ok((secret as string).length > 0, `add returned an empty ${KEY_SECRET} secret: ${result.stdout}`);
  return { agentKeyId: agentKeyId as string, secret: secret as string };
}

/**
 * Perform one CLI read with the agent key as the bearer. Reuses the JWT
 * identity's hydrated state dir (so the workspace context already exists on
 * disk) and swaps ONLY the token. Returns the raw run result; the caller
 * asserts the outcome (an `agt_a` key is rejected on control-plane reads —
 * see this file's header — so the caller asserts rejection, not success).
 */
export async function readWithAgentKey(
  baseEnv: Env,
  agentKeySecret: string,
  args: ReadonlyArray<string>,
): Promise<RunResult> {
  const keyEnv = composeEnv({
    [ENV_TOKEN]: agentKeySecret,
    [ENV_API_URL]: baseEnv[ENV_API_URL],
    [ENV_STATE_DIR]: baseEnv[ENV_STATE_DIR],
    [ENV_NO_COLOR]: NO_COLOR_ON,
  });
  const result = await runAgentctl([...args, FLAG_JSON], { env: keyEnv, stdin: "" });
  // The bearer here IS the agent key, not the JWT — but the key must never
  // echo into stderr/stdout in plaintext.
  assertNoSecretLeak(result, agentKeySecret);
  return result;
}

/** `agent-key delete <id> --json` — best-effort revoke (teardown + the test). */
export async function revokeAgentKey(env: Env, agentKeyId: string): Promise<RunResult> {
  return runAgentctl([CMD_AGENT_KEY, SUB_DELETE, agentKeyId, FLAG_JSON], { env, stdin: "" });
}
