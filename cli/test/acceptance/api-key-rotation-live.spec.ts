/**
 * api-key-rotation-live — an automation key minted, used, revoked and removed
 * from the terminal.
 *
 * The walk a person runs to rotate the key a nightly job holds:
 *
 *   - `api-keys create` reveals an `agt_t` key once; `api-keys list` shows its
 *     last four characters, the ending the dashboard masks it to, and never
 *     the key itself
 *   - the key alone, exported as AGENTSFLEET_API_KEY in a terminal holding no
 *     login, answers `whoami` as the person who minted it
 *   - `api-keys delete` refuses a key that still authenticates
 *   - `api-keys revoke` stops the key at its next request with the revoked-key
 *     code, not the generic one, so a job knows to stop retrying
 *   - `api-keys delete` removes the revoked row, after which the key names
 *     nothing at all
 *
 * The codes are the daemon's: `afd_auth::Error::TenantKeyRevoked` answers
 * `APIKEY_REVOKED`, and `afd_tenant::apikey` raises the two revoke and delete
 * refusals; `rustd/crates/foundation/af_core/src/error_code/auth.rs` declares
 * all of them.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, refusalCode } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { API_KEY_ENV } from "../../src/constants/env.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-api-key-" as const;
const KEY_STATE_DIR_PREFIX = "agentsfleet-api-key-only-" as const;
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const API_KEYS = "api-keys" as const;
const WHOAMI = ["whoami", JSON_FLAG] as const;
// `afd_auth::credential::TENANT_API_KEY_PREFIX` and its 64-hex body.
const TENANT_KEY_SHAPE = /^agt_t[0-9a-f]{64}$/u;
// `afd_api_wire::tenant::ApiKeySummary::display_suffix`: the final four characters.
const DISPLAY_SUFFIX_LENGTH = 4;
// `credential_class` and the source `whoami` reports for an exported key
// (src/commands/auth/whoami.ts).
const CREDENTIAL_TENANT_API_KEY = "tenant_api_key" as const;
const SOURCE_ENVIRONMENT = "environment" as const;
const CODE_KEY_REVOKED = "UZ-APIKEY-004" as const;
const CODE_KEY_ALREADY_REVOKED = "UZ-APIKEY-006" as const;
const CODE_KEY_MUST_REVOKE_FIRST = "UZ-APIKEY-008" as const;
// A key whose row is gone matches nothing, and an unknown credential answers
// the generic refusal by design (afd_auth/src/error.rs).
const CODE_UNAUTHORIZED = "UZ-AUTH-002" as const;

const MintedSchema = Schema.Struct({ id: Schema.String, key_name: Schema.String, key: Schema.String });
const KeyRowSchema = Schema.Struct({
  id: Schema.String,
  key_name: Schema.String,
  display_suffix: Schema.NullOr(Schema.String),
  active: Schema.Boolean,
  revoked_at: Schema.NullOr(Schema.String),
});
const KeyListSchema = Schema.Struct({ items: Schema.Array(KeyRowSchema) });
const RevokedSchema = Schema.Struct({ id: Schema.String, active: Schema.Boolean, revoked_at: Schema.String });
const DeletedSchema = Schema.Struct({ deleted: Schema.Literal(true), id: Schema.String });
const IdentitySchema = Schema.Struct({
  authenticated: Schema.Literal(true),
  user_id: Schema.String,
  credential: Schema.String,
  source: Schema.String,
});

if (!isLive) {
  describe("api-key-rotation-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("api-key-rotation-live — create, use, revoke and remove an automation key", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let keyStateDir = "";
    let env: Record<string, string> = {};
    const keyName = `${ACCEPTANCE_RUN_PREFIX}-nightly-export`;
    let keyId = "";
    let keyValue = "";
    let ownerId = "";
    let removed = false;

    async function run(
      runEnv: Readonly<Record<string, string>>,
      args: ReadonlyArray<string>,
      revealsKey = false,
    ): Promise<RunResult> {
      const result = await runFleetctl(args, { env: runEnv });
      assertNoSecretLeak(result, sessionJwt);
      // Only create reveals the key; every other answer, refusals included,
      // must not echo it back.
      if (!revealsKey) assertNoSecretLeak(result, keyValue);
      return result;
    }

    // A terminal holding nothing but the exported key: its own empty state
    // directory, so no stored login can answer in the key's place.
    const keyOnlyEnv = (): Record<string, string> => composeEnv({
      AGENTSFLEET_API_URL: apiUrl,
      AGENTSFLEET_STATE_DIR: keyStateDir,
      [API_KEY_ENV]: keyValue,
      NO_COLOR,
    });

    async function whoami(runEnv: Readonly<Record<string, string>>): Promise<typeof IdentitySchema.Type> {
      const result = await run(runEnv, WHOAMI);
      assert.equal(result.code, 0, `whoami failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(IdentitySchema)(JSON.parse(result.stdout.trim()));
    }

    async function listedKey(): Promise<typeof KeyRowSchema.Type | undefined> {
      const result = await run(env, [API_KEYS, "list", JSON_FLAG]);
      assert.equal(result.code, 0, `api-keys list failed: ${result.stderr}`);
      const page = Schema.decodeUnknownSync(KeyListSchema)(JSON.parse(result.stdout.trim()));
      return page.items.find((row) => row.id === keyId);
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), { email: resolveFixtureEmail("regular") });
      sessionJwt = minted.sessionJwt;
      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      keyStateDir = await fs.mkdtemp(path.join(os.tmpdir(), KEY_STATE_DIR_PREFIX));
      env = composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR });
      await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
    });

    afterAll(async () => {
      try {
        // A walk that failed midway must not leave a live automation key behind.
        if (keyId && !removed) {
          await runFleetctl([API_KEYS, "revoke", keyId, JSON_FLAG], { env });
          const deleted = await runFleetctl([API_KEYS, "delete", keyId, JSON_FLAG], { env });
          assert.equal(deleted.code, 0, `api-key cleanup failed: ${deleted.stderr}`);
        }
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
        if (keyStateDir) await fs.rm(keyStateDir, { recursive: true, force: true });
      }
    });

    it("`api-keys create` reveals an agt_t key once, and the list shows only its ending", async () => {
      const created = await run(env, [API_KEYS, "create", "--name", keyName, JSON_FLAG], true);
      assert.equal(created.code, 0, `api-keys create failed: ${created.stderr}`);
      const mint = Schema.decodeUnknownSync(MintedSchema)(JSON.parse(created.stdout.trim()));
      assert.match(mint.key, TENANT_KEY_SHAPE, "create must reveal a tenant API key");
      assert.equal(mint.key_name, keyName);
      keyId = mint.id;
      keyValue = mint.key;

      const row = await listedKey();
      assert.ok(row, `api-keys list does not show the key just created: ${keyId}`);
      assert.equal(row.active, true);
      assert.equal(row.revoked_at, null);
      assert.equal(row.display_suffix, keyValue.slice(-DISPLAY_SUFFIX_LENGTH),
        "the masked ending a person matches against must be the key's own last characters");
    });

    it("the key alone authenticates `whoami` as the person who minted it", async () => {
      assert.ok(keyValue, "no key was created");
      const owner = await whoami(env);
      const viaKey = await whoami(keyOnlyEnv());
      assert.equal(viaKey.credential, CREDENTIAL_TENANT_API_KEY);
      assert.equal(viaKey.source, SOURCE_ENVIRONMENT);
      assert.equal(viaKey.user_id, owner.user_id, "a tenant key acts as the person who minted it");
      ownerId = owner.user_id;
    });

    it("`api-keys delete` refuses a key that still authenticates, and the key keeps working", async () => {
      assert.ok(ownerId, "the key never authenticated");
      const refused = await run(env, [API_KEYS, "delete", keyId, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `delete of an active key: ${refused.stderr}`);
      assert.equal(refusalCode(refused), CODE_KEY_MUST_REVOKE_FIRST);
      assert.equal((await whoami(keyOnlyEnv())).user_id, ownerId, "a refused delete must leave the key usable");
      assert.equal((await listedKey())?.active, true);
    });

    it("`api-keys revoke` stops the key at its next request with the revoked-key code", async () => {
      assert.ok(keyId, "no key was created");
      const revoked = await run(env, [API_KEYS, "revoke", keyId, JSON_FLAG]);
      assert.equal(revoked.code, 0, `api-keys revoke failed: ${revoked.stderr}`);
      const answer = Schema.decodeUnknownSync(RevokedSchema)(JSON.parse(revoked.stdout.trim()));
      assert.equal(answer.id, keyId);
      assert.equal(answer.active, false);

      const refused = await run(keyOnlyEnv(), WHOAMI);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a revoked key still answered: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_KEY_REVOKED);

      const row = await listedKey();
      assert.equal(row?.active, false, `the list still shows the key active: ${JSON.stringify(row)}`);
      assert.equal(row?.revoked_at, answer.revoked_at);
    });

    it("a second `api-keys revoke` is refused as already revoked", async () => {
      assert.ok(keyId, "no key was created");
      const again = await run(env, [API_KEYS, "revoke", keyId, JSON_FLAG]);
      assert.equal(again.code, EXIT_CODE.ServerError, `a second revoke succeeded: ${again.stdout}`);
      assert.equal(refusalCode(again), CODE_KEY_ALREADY_REVOKED);
    });

    it("`api-keys delete` removes the revoked key, and the key then names nothing", async () => {
      assert.ok(keyId, "no key was created");
      const deleted = await run(env, [API_KEYS, "delete", keyId, JSON_FLAG]);
      assert.equal(deleted.code, 0, `api-keys delete failed: ${deleted.stderr}`);
      assert.equal(Schema.decodeUnknownSync(DeletedSchema)(JSON.parse(deleted.stdout.trim())).id, keyId);
      removed = true;

      assert.equal(await listedKey(), undefined, "api-keys list still shows a deleted key");
      const refused = await run(keyOnlyEnv(), WHOAMI);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a deleted key still answered: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_UNAUTHORIZED);
    });
  });
}
