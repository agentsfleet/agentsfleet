// Shared fixtures and doubles for the secret vault acceptance suites.
//
// Extracted when secret-vault.spec.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import crypto from "node:crypto";
import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV, UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl, type RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, assertNoConnectionError } from "./fixtures/negatives.ts";
import { beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { sweepSecrets } from "./fixtures/secret-ops.ts";
import { API_URL_ENV, STATE_DIR_ENV } from "../../src/constants/env.ts";

export const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
export const isLive = target.startsWith("https://");

// --- command/flag/key constants (RULE UFS) ---------------------------------
export const CMD_SECRET = "secret" as const;
export const SUB_CREATE = "create" as const;
export const SUB_SHOW = "show" as const;
export const SUB_LIST = "list" as const;
export const SUB_DELETE = "delete" as const;
export const SUB_UPDATE = "update" as const;
export const FLAG_DATA = "--data" as const;
export const FLAG_FORCE = "--force" as const;
export const FLAG_JSON = "--json" as const;

export const KEY_SECRETS = "secrets" as const;
export const KEY_NAME = "name" as const;
export const KEY_STATUS = "status" as const;
export const KEY_EXISTS = "exists" as const;
export const KEY_REASON = "reason" as const;

export const STATUS_STORED = "stored" as const;
export const STATUS_SKIPPED = "skipped" as const;
export const STATUS_DELETED = "deleted" as const;
export const STATUS_UPDATED = "updated" as const;
export const REASON_ALREADY_EXISTS = "already_exists" as const;

export const ENV_NO_COLOR = "NO_COLOR" as const;
export const NO_COLOR_ON = "1" as const;

export const STATE_DIR_PREFIX = "agentsfleet-secretvault-" as const;
export const UNKNOWN_NAME_SUFFIX = "ghost" as const;

// Custom-endpoint typed secret-create form.
export const FLAG_PROVIDER = "--provider" as const;
export const FLAG_BASE_URL = "--base-url" as const;
export const FLAG_API_KEY = "--api-key" as const;
export const FLAG_MODEL = "--model" as const;
export const CUSTOM_ENDPOINT_MODEL = "qwen2.5-acceptance" as const;
export const CUSTOM_BASE_URL = "https://vllm.acceptance.example/v1" as const;
export const NON_HTTPS_BASE_URL = "http://vllm.acceptance.example/v1" as const;

// A quoted JSON scalar — valid JSON, but not the object `create` requires, so the
// client-side payload guard must reject it before any network call.
export const SCALAR_PAYLOAD = '"just-a-string"' as const;

export const ENC_HEX = "hex" as const;
export const SECRET_ENTROPY_BYTES = 18 as const;

// Secret values planted in the payload — every assertion below proves these
// never reach a captured stream. Distinct, high-entropy, easy to grep for.
export const SECRET_TOKEN_VALUE = `sk-live-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
export const SECRET_PASSWORD_VALUE = `pw-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
// The custom-endpoint secret's api_key is also a planted secret — every
// leak assertion below proves it never reaches a captured stream (VLT).
export const CUSTOM_API_KEY_VALUE = `sk-custom-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
export const SECRET_REPLACED_VALUE = `sk-replaced-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
export const SECRET_VALUES: ReadonlyArray<string> = [
  SECRET_TOKEN_VALUE,
  SECRET_PASSWORD_VALUE,
  CUSTOM_API_KEY_VALUE,
  SECRET_REPLACED_VALUE,
];

export const secretName = (label: string): string => `${ACCEPTANCE_RUN_PREFIX}-${label}`;

export const secretPayload = (): string =>
  JSON.stringify({ api_token: SECRET_TOKEN_VALUE, password: SECRET_PASSWORD_VALUE });

export interface SecretListEnvelope {
  readonly secrets?: ReadonlyArray<{ readonly name?: string }>;
}

export function parseJson<T>(stdout: string, label: string): T {
  const trimmed = stdout.trim();
  try {
    return JSON.parse(trimmed) as T;
  } catch {
    throw new Error(`${label}: stdout was not parseable JSON: ${trimmed}`);
  }
}

export function listIncludesName(envelope: SecretListEnvelope, name: string): boolean {
  const rows = Array.isArray(envelope.secrets) ? envelope.secrets : [];
  return rows.some((row) => row.name === name);
}

/** No secret payload value (nor the JWT) may surface in any stream. */
export function assertNoSecretMaterialLeak(captured: RunResult, jwt: string): void {
  assertNoSecretLeak(captured, jwt);
  const merged = `${captured.stdout}\n${captured.stderr}`;
  for (const secret of SECRET_VALUES) {
    if (merged.includes(secret)) {
      throw new Error(
        `secret material leaked into captured stdout/stderr: ${captured.stdout}\n${captured.stderr}`,
      );
    }
  }
}

/** One seeded-credentials vault session, with its own setup and teardown.
 *
 * A factory rather than module state: the suites read it through getters, so a
 * value `beforeAll` has not filled yet reads as the empty string it always did
 * rather than as a stale import-time snapshot. It registers its own
 * `beforeAll`/`afterAll`, so calling it inside a `describe` is the whole wiring.
 *
 * It lives here because secret-vault.spec.ts passed the repository's 350-line
 * cap and this scaffold is the half with no assertions in it. One factory call
 * per file also keeps the live lane honest: a second spec file would mint a
 * second Clerk session against the real API for no extra coverage.
 */
export interface VaultSession {
  readonly apiUrl: () => string;
  readonly token: () => string;
  readonly workspaceId: () => string;
  readonly run: (
    args: ReadonlyArray<string>,
    extraEnv?: Record<string, string>,
  ) => Promise<RunResult>;
  readonly runUnroutable: (args: ReadonlyArray<string>) => Promise<RunResult>;
}

export function vaultSession(): VaultSession {
  let apiUrl = "";
  let sessionJwt = "";
  let stateDir = "";
  let env: Record<string, string> = {};
  let workspaceId = "";

  async function run(
    args: ReadonlyArray<string>,
    extraEnv?: Record<string, string>,
  ): Promise<RunResult> {
    const composed = extraEnv ? { ...env, ...extraEnv } : env;
    const result = await runFleetctl(args, { env: composed, stdin: "" });
    assertNoSecretMaterialLeak(result, sessionJwt);
    return result;
  }

  // Run against an unroutable API on the already-hydrated state dir: a
  // client-side guard must reject the args before any network call, so an
  // observed connection error would prove the guard was bypassed.
  async function runUnroutable(args: ReadonlyArray<string>): Promise<RunResult> {
    const unroutable = { ...env, [API_URL_ENV]: UNROUTABLE_API_URL };
    const result = await runFleetctl(args, { env: unroutable, stdin: "" });
    assert.notEqual(result.code, 0, `expected non-zero; stdout=${result.stdout}`);
    assertNoConnectionError(result, args);
    assertNoSecretMaterialLeak(result, sessionJwt);
    return result;
  }

  beforeAll(async () => {
    apiUrl = resolveAcceptanceEnv().apiUrl;
    const clerkSecret = resolveClerkSecret();
    const email = resolveFixtureEmail("regular");
    const minted = await attachJwt(clerkSecret, { email });
    sessionJwt = minted.sessionJwt;

    stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
    env = composeEnv({
      [API_URL_ENV]: apiUrl,
      [STATE_DIR_ENV]: stateDir,
      [ENV_NO_COLOR]: NO_COLOR_ON,
    });
    const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
    workspaceId = hydrated.currentWorkspaceId;
  });

  afterAll(async () => {
    if (apiUrl && sessionJwt && workspaceId) {
      try {
        await sweepSecrets(
          { apiUrl, token: sessionJwt, workspaceId },
          { runPrefix: ACCEPTANCE_RUN_PREFIX },
        );
      } catch {
        /* best-effort teardown — never throw out of afterAll */
      }
    }
    if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
  });

  return {
    apiUrl: () => apiUrl,
    token: () => sessionJwt,
    workspaceId: () => workspaceId,
    run,
    runUnroutable,
  };
}
