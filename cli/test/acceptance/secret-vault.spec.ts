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
 *   - create of an existing name is skipped (the endpoint claims a free name
 *     and never overwrites); --force no longer exists and is rejected at parse
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
import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../../src/constants/custom-endpoint.ts";
import { sweepSecrets } from "./fixtures/secret-ops.ts";
import {
  isLive,
  CMD_SECRET,
  SUB_CREATE,
  SUB_SHOW,
  SUB_LIST,
  SUB_DELETE,
  SUB_UPDATE,
  FLAG_DATA,
  FLAG_FORCE,
  FLAG_JSON,
  KEY_SECRETS,
  KEY_NAME,
  KEY_STATUS,
  KEY_EXISTS,
  KEY_REASON,
  STATUS_STORED,
  STATUS_SKIPPED,
  STATUS_DELETED,
  STATUS_UPDATED,
  REASON_ALREADY_EXISTS,
  UNKNOWN_NAME_SUFFIX,
  FLAG_PROVIDER,
  FLAG_BASE_URL,
  FLAG_API_KEY,
  FLAG_MODEL,
  CUSTOM_ENDPOINT_MODEL,
  CUSTOM_BASE_URL,
  NON_HTTPS_BASE_URL,
  SCALAR_PAYLOAD,
  CUSTOM_API_KEY_VALUE,
  SECRET_REPLACED_VALUE,
  SECRET_VALUES,
  secretName,
  secretPayload,
  type SecretListEnvelope,
  parseJson,
  listIncludesName,
  vaultSession,
} from "./helpers-secret-vault.ts";

if (!isLive) {
  describe("secret-vault.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("secret-vault — round-trip (seeded-credentials session)", () => {
    const session = vaultSession();
    const { run, runUnroutable } = session;
    const roundTripName = secretName("roundtrip");


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

      it("update replaces the whole body in one call — the name never lapses", async () => {
        const replacement = JSON.stringify({ api_token: SECRET_REPLACED_VALUE });
        const result = await run([
          CMD_SECRET, SUB_UPDATE, roundTripName, FLAG_DATA, replacement, FLAG_JSON,
        ]);
        assert.equal(result.code, 0, `update exited ${result.code}: ${result.stderr}`);
        const parsed = parseJson<Record<string, unknown>>(result.stdout, SUB_UPDATE);
        assert.equal(parsed[KEY_STATUS], STATUS_UPDATED, `unexpected update status: ${result.stdout}`);
        assert.equal(parsed[KEY_NAME], roundTripName, `update echoed wrong name: ${result.stdout}`);
        // Still resolvable under the same name immediately after the replace.
        const show = await run([CMD_SECRET, SUB_SHOW, roundTripName, FLAG_JSON]);
        assert.equal(show.code, 0, `post-update show exited ${show.code}: ${show.stderr}`);
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

    describe("name claim", () => {
      const upsertName = secretName("upsert");

      afterAll(async () => {
        await run([CMD_SECRET, SUB_DELETE, upsertName, FLAG_JSON]).catch(() => undefined);
      });

      it("first create stores; a repeat is skipped; --force is gone and never reaches the API", async () => {
        const first = await run([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        assert.equal(first.code, 0, `first create exited ${first.code}: ${first.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(first.stdout, "first-create")[KEY_STATUS],
          STATUS_STORED,
        );

        // The live daemon answers UZ-VAULT-005 and writes nothing. The CLI
        // reports that as a skip so a re-run of a provisioning script is quiet.
        const repeatCreate = await run([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        assert.equal(repeatCreate.code, 0, `repeat create exited ${repeatCreate.code}`);
        const reParsed = parseJson<Record<string, unknown>>(repeatCreate.stdout, "repeat-create");
        assert.equal(reParsed[KEY_STATUS], STATUS_SKIPPED, `expected skipped: ${repeatCreate.stdout}`);
        assert.equal(reParsed[KEY_REASON], REASON_ALREADY_EXISTS, `expected reason: ${repeatCreate.stdout}`);

        // Replacing a value is delete-then-create; the flag that used to claim
        // otherwise is rejected before anything is sent.
        await runUnroutable([
          CMD_SECRET, SUB_CREATE, upsertName, FLAG_DATA, secretPayload(), FLAG_FORCE, FLAG_JSON,
        ]);
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

      it("update of an unknown name fails without creating it", async () => {
        const ghost = secretName(UNKNOWN_NAME_SUFFIX);
        const result = await run([
          CMD_SECRET, SUB_UPDATE, ghost, FLAG_DATA, secretPayload(), FLAG_JSON,
        ]);
        assert.notEqual(result.code, 0, `unknown update should fail; stdout=${result.stdout}`);
        const list = await run([CMD_SECRET, SUB_LIST, FLAG_JSON]);
        const parsed = parseJson<SecretListEnvelope>(list.stdout, SUB_LIST);
        assert.ok(!listIncludesName(parsed, ghost), `failed update created ${ghost}: ${list.stdout}`);
      });

      it("update without --data is rejected client-side (no network)", async () => {
        await runUnroutable([CMD_SECRET, SUB_UPDATE, secretName("upd-nodata"), FLAG_JSON]);
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
    // flag validator with NO network call.
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
          FLAG_MODEL, CUSTOM_ENDPOINT_MODEL,
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
          FLAG_MODEL, CUSTOM_ENDPOINT_MODEL,
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
          {
            apiUrl: session.apiUrl(),
            token: session.token(),
            workspaceId: session.workspaceId(),
          },
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
