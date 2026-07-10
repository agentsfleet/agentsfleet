/**
 * Secret-vault round-trip (live, seeded-credentials session).
 *
 * Walks the workspace secret vault end to end against the live DEV API:
 *   create (name + JSON object via --data) → list --json contains it →
 *   show --json reports exists:true and NEVER echoes the secret bytes →
 *   delete → list --json excludes it → show --json reports exists:false.
 *
 * Plus the negative edges that gate the slice:
 *   - create without --data fails client-side (no network)
 *   - create with a non-object payload fails client-side
 *   - create of an existing name without --force is skipped, --force overwrites
 *   - show of an unknown name exits non-zero, exists:false
 *   - secret material never appears in any captured stream (assertNoSecretLeak
 *     also fires against the minted JWT after every spawn)
 *
 * Every secret is prefix-scoped with ACCEPTANCE_RUN_PREFIX; afterAll
 * sweeps any leftovers straight through the API so a crash can't strand a
 * named secret in the shared tenant. No assertion claims global emptiness —
 * the invariant is "none of MY run's secrets remain".
 *
 * Live-only: registers real tests only when AGENTSFLEET_ACCEPTANCE_TARGET is
 * an https URL; otherwise the suite skips cleanly (CI runs it live).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV, UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoConnectionError, assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { sweepSecrets } from "./fixtures/secret-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// --- command/flag/key constants (RULE UFS) ---------------------------------
const CMD_SECRET = "secret" as const;
const SUB_CREATE = "create" as const;
const SUB_SHOW = "show" as const;
const SUB_LIST = "list" as const;
const SUB_DELETE = "delete" as const;
const FLAG_DATA = "--data" as const;
const FLAG_FORCE = "--force" as const;
const FLAG_JSON = "--json" as const;

const KEY_SECRETS = "secrets" as const;
const KEY_NAME = "name" as const;
const KEY_STATUS = "status" as const;
const KEY_EXISTS = "exists" as const;
const KEY_REASON = "reason" as const;

const STATUS_STORED = "stored" as const;
const STATUS_OVERWRITTEN = "overwritten" as const;
const STATUS_SKIPPED = "skipped" as const;
const STATUS_DELETED = "deleted" as const;
const REASON_ALREADY_EXISTS = "already_exists" as const;

const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
const ENV_NO_COLOR = "NO_COLOR" as const;
const NO_COLOR_ON = "1" as const;

const STATE_DIR_PREFIX = "agentsfleet-secretvault-" as const;
const UNKNOWN_NAME_SUFFIX = "ghost" as const;

// Custom-endpoint typed secret-create form.
const FLAG_PROVIDER = "--provider" as const;
const FLAG_BASE_URL = "--base-url" as const;
const FLAG_API_KEY = "--api-key" as const;
const OPENAI_COMPATIBLE_PROVIDER = "openai-compatible" as const;
const CUSTOM_BASE_URL = "https://vllm.acceptance.example/v1" as const;
const NON_HTTPS_BASE_URL = "http://vllm.acceptance.example/v1" as const;

// A quoted JSON scalar — valid JSON, but not the object `create` requires, so the
// client-side payload guard must reject it before any network call.
const SCALAR_PAYLOAD = '"just-a-string"' as const;

const ENC_HEX = "hex" as const;
const SECRET_ENTROPY_BYTES = 18 as const;

// Secret values planted in the payload — every assertion below proves these
// never reach a captured stream. Distinct, high-entropy, easy to grep for.
const SECRET_TOKEN_VALUE = `sk-live-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
const SECRET_PASSWORD_VALUE = `pw-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
// The custom-endpoint secret's api_key is also a planted secret — every
// leak assertion below proves it never reaches a captured stream (VLT).
const CUSTOM_API_KEY_VALUE = `sk-custom-${crypto.randomBytes(SECRET_ENTROPY_BYTES).toString(ENC_HEX)}`;
const SECRET_VALUES: ReadonlyArray<string> = [
  SECRET_TOKEN_VALUE,
  SECRET_PASSWORD_VALUE,
  CUSTOM_API_KEY_VALUE,
];

const secretName = (label: string): string => `${ACCEPTANCE_RUN_PREFIX}-${label}`;

const secretPayload = (): string =>
  JSON.stringify({ api_token: SECRET_TOKEN_VALUE, password: SECRET_PASSWORD_VALUE });

interface SecretListEnvelope {
  readonly secrets?: ReadonlyArray<{ readonly name?: string }>;
}

function parseJson<T>(stdout: string, label: string): T {
  const trimmed = stdout.trim();
  try {
    return JSON.parse(trimmed) as T;
  } catch {
    throw new Error(`${label}: stdout was not parseable JSON: ${trimmed}`);
  }
}

function listIncludesName(envelope: SecretListEnvelope, name: string): boolean {
  const rows = Array.isArray(envelope.secrets) ? envelope.secrets : [];
  return rows.some((row) => row.name === name);
}

/** No secret payload value (nor the JWT) may surface in any stream. */
function assertNoSecretMaterialLeak(captured: RunResult, jwt: string): void {
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

if (!isLive) {
  describe("secret-vault.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("secret-vault — round-trip (seeded-credentials session)", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";

    const roundTripName = secretName("roundtrip");

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
      const unroutable = { ...env, [ENV_API_URL]: UNROUTABLE_API_URL };
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
        [ENV_API_URL]: apiUrl,
        [ENV_STATE_DIR]: stateDir,
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

    describe("happy-path round-trip", () => {
      it("create stores a named JSON secret", async () => {
        const result = await run([
          CMD_SECRET, SUB_CREATE, roundTripName, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        assert.equal(result.code, 0, `create exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, SUB_CREATE);
        assert.equal(parsed[KEY_STATUS], STATUS_STORED, `unexpected create status: ${result.stdout}`);
        assert.equal(parsed[KEY_NAME], roundTripName, `create echoed wrong name: ${result.stdout}`);
      });

      it("list --json contains the stored secret", async () => {
        const result = await run([CMD_SECRET, SUB_LIST, FLAG_JSON]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<SecretListEnvelope>(result.stdout, SUB_LIST);
        assert.ok(KEY_SECRETS in parsed, `list missing ${KEY_SECRETS}: ${result.stdout}`);
        assert.ok(Array.isArray(parsed.secrets), `${KEY_SECRETS} not an array: ${result.stdout}`);
        assert.ok(
          listIncludesName(parsed, roundTripName),
          `list omitted ${roundTripName}: ${result.stdout}`,
        );
      });

      it("show --json confirms existence without printing secret bytes", async () => {
        const result = await run([CMD_SECRET, SUB_SHOW, roundTripName, FLAG_JSON]);
        assert.equal(result.code, 0, `show exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, SUB_SHOW);
        assert.equal(parsed[KEY_NAME], roundTripName, `show echoed wrong name: ${result.stdout}`);
        assert.equal(parsed[KEY_EXISTS], true, `show reported missing: ${result.stdout}`);
        // Belt-and-braces: the envelope must carry no field whose value is a
        // planted secret (assertNoSecretMaterialLeak already covers raw streams).
        for (const secret of SECRET_VALUES) {
          assert.ok(!result.stdout.includes(secret), `show leaked secret: ${result.stdout}`);
        }
      });

      it("delete removes the secret", async () => {
        const result = await run([CMD_SECRET, SUB_DELETE, roundTripName, FLAG_JSON]);
        assert.equal(result.code, 0, `delete exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, SUB_DELETE);
        assert.equal(parsed[KEY_STATUS], STATUS_DELETED, `unexpected delete status: ${result.stdout}`);
        assert.equal(parsed[KEY_NAME], roundTripName, `delete echoed wrong name: ${result.stdout}`);
      });

      it("list --json no longer contains the deleted secret", async () => {
        const result = await run([CMD_SECRET, SUB_LIST, FLAG_JSON]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<SecretListEnvelope>(result.stdout, SUB_LIST);
        assert.ok(
          !listIncludesName(parsed, roundTripName),
          `deleted secret still present: ${result.stdout}`,
        );
      });

      it("show --json after delete reports exists:false and exits non-zero", async () => {
        const result = await run([CMD_SECRET, SUB_SHOW, roundTripName, FLAG_JSON]);
        assert.notEqual(result.code, 0, `show of deleted name should fail; stdout=${result.stdout}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, "show-missing");
        assert.equal(parsed[KEY_EXISTS], false, `expected exists:false: ${result.stdout}`);
        assert.equal(parsed[KEY_NAME], roundTripName, `show echoed wrong name: ${result.stdout}`);
      });
    });

    describe("upsert guard", () => {
      const upsertName = secretName("upsert");

      afterAll(async () => {
        await run([CMD_SECRET, SUB_DELETE, upsertName, FLAG_JSON]).catch(() => undefined);
      });

      it("first create stores; repeat create without --force is skipped; --force overwrites", async () => {
        const first = await run([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        assert.equal(first.code, 0, `first create exited ${first.code}: ${first.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(first.stdout, "first-create")[KEY_STATUS],
          STATUS_STORED,
        );

        const repeatCreate = await run([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        const reParsed = parseJson<Record<string, unknown>>(repeatCreate.stdout, "repeat-create");
        assert.equal(reParsed[KEY_STATUS], STATUS_SKIPPED, `expected skipped: ${repeatCreate.stdout}`);
        assert.equal(reParsed[KEY_REASON], REASON_ALREADY_EXISTS, `expected reason: ${repeatCreate.stdout}`);

        const forced = await run([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_FORCE, FLAG_JSON,
        ]);
        assert.equal(forced.code, 0, `forced create exited ${forced.code}: ${forced.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(forced.stdout, "forced-create")[KEY_STATUS],
          STATUS_OVERWRITTEN,
          `expected overwritten: ${forced.stdout}`,
        );
      });
    });

    describe("negative edges", () => {
      it("show of an unknown name reports exists:false and exits non-zero", async () => {
        const ghost = secretName(UNKNOWN_NAME_SUFFIX);
        const result = await run([CMD_SECRET, SUB_SHOW, ghost, FLAG_JSON]);
        assert.notEqual(result.code, 0, `unknown show should fail; stdout=${result.stdout}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, "show-unknown");
        assert.equal(parsed[KEY_EXISTS], false, `expected exists:false: ${result.stdout}`);
      });

      it("create without --data is rejected client-side (no network)", async () => {
        await runUnroutable([CMD_SECRET, SUB_CREATE, secretName("nodata"), FLAG_JSON]);
      });

      it("create with a non-object payload is rejected client-side (no network)", async () => {
        await runUnroutable([
          CMD_SECRET, SUB_CREATE, secretName("scalar"), FLAG_DATA, SCALAR_PAYLOAD, FLAG_JSON,
        ]);
      });
    });

    // Custom OpenAI-compatible endpoint secret — the typed secret-create
    // form stores provider + base_url; a non-https URL is rejected by the
    // commander validator with NO network call.
    describe("custom OpenAI-compatible endpoint", () => {
      const customName = secretName("custom-endpoint");

      afterAll(async () => {
        await run([CMD_SECRET, SUB_DELETE, customName, FLAG_JSON]).catch(() => undefined);
      });

      it("create --provider openai-compatible --base-url <https> --api-key <key> stores it", async () => {
        const result = await run([
          CMD_SECRET, SUB_CREATE, customName,
          FLAG_PROVIDER, OPENAI_COMPATIBLE_PROVIDER,
          FLAG_BASE_URL, CUSTOM_BASE_URL,
          FLAG_API_KEY, CUSTOM_API_KEY_VALUE,
          FLAG_JSON,
        ]);
        assert.equal(result.code, 0, `custom create exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, "custom-create");
        assert.equal(parsed[KEY_STATUS], STATUS_STORED, `unexpected custom create status: ${result.stdout}`);
        assert.equal(parsed[KEY_NAME], customName, `custom create echoed wrong name: ${result.stdout}`);
      });

      it("list --json contains the custom-endpoint secret", async () => {
        const result = await run([CMD_SECRET, SUB_LIST, FLAG_JSON]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<SecretListEnvelope>(result.stdout, "list-custom");
        assert.ok(listIncludesName(parsed, customName), `list omitted ${customName}: ${result.stdout}`);
      });

      it("a non-https --base-url is rejected client-side (non-zero exit, no network)", async () => {
        await runUnroutable([
          CMD_SECRET, SUB_CREATE, secretName("custom-bad"),
          FLAG_PROVIDER, OPENAI_COMPATIBLE_PROVIDER,
          FLAG_BASE_URL, NON_HTTPS_BASE_URL,
          FLAG_API_KEY, CUSTOM_API_KEY_VALUE,
          FLAG_JSON,
        ]);
      });
    });

    // Prefix-scoped post-teardown emptiness — shared DEV tenants carry
    // residual secrets from other runs, so the invariant is "none of MY
    // run's remain", never global emptiness.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await sweepSecrets(
          { apiUrl, token: sessionJwt, workspaceId },
          { runPrefix: ACCEPTANCE_RUN_PREFIX },
        );
      });

      it("list --json: no secret matches ACCEPTANCE_RUN_PREFIX", async () => {
        const result = await run([CMD_SECRET, SUB_LIST, FLAG_JSON]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<SecretListEnvelope>(result.stdout, "list-final");
        const rows = Array.isArray(parsed.secrets) ? parsed.secrets : [];
        const mine = rows.filter(
          (row) => typeof row.name === "string" && row.name.startsWith(ACCEPTANCE_RUN_PREFIX),
        );
        assert.equal(
          mine.length,
          0,
          `expected zero secrets starting with ${ACCEPTANCE_RUN_PREFIX}; got ${JSON.stringify(mine)}`,
        );
      });
    });
  });
}
