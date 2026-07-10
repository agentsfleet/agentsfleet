/**
 * Referential-integrity / cross-resource acceptance scenarios (live,
 * seeded-credentials session). Mirrors lifecycle-with-token / secret-vault
 * / fleet-key-mutation: mint a Clerk session JWT, hydrate workspaces.json from
 * the API, then walk cross-resource deletes whose outcome the suite DISCOVERS
 * and DOCUMENTS against api-dev rather than presumes. The load-bearing rule
 * for each scenario is restated as an inline comment at its `describe` block:
 *
 *   (a) delete a secret a tenant provider references → refused-conflict OR
 *       cascade-with-credential_missing disjunction; baseline restored on fail.
 *   (b) `workspace delete` is LOCAL-only (no server DELETE) → the server fleet
 *       survives and stays reachable via `list --workspace-id`.
 *   (c) an `agt_a…` fleet key is NOT a control-plane credential — it is rejected
 *       on a control-plane read (`fleet-key list`) both before AND after revoke.
 *
 * Prefix-scoped: every fleet + secret + key is ACCEPTANCE_RUN_PREFIX-named
 * and cleaned in afterAll; no assertion claims global emptiness. Live-only:
 * real tests register only when AGENTSFLEET_ACCEPTANCE_TARGET is an https URL;
 * otherwise the suite skips cleanly (CI runs it live).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import { sweepSecrets } from "./fixtures/secret-ops.ts";
import {
  TENANT_PROVIDER_MODE,
  restoreProviderBaseline,
  showProvider,
} from "./fixtures/tenant-provider-ops.ts";
import type { ProviderSnapshot } from "./fixtures/tenant-provider-ops.ts";
import {
  AGENT_KEY_SECRET_PREFIX,
  REJECTED_AUTH_RE,
  assertSecretDeleteDisjunction,
  mintFleetKey,
  readWithFleetKey,
  revokeFleetKey,
} from "./fixtures/referential-ops.ts";
import type { MintedFleetKey } from "./fixtures/referential-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// --- command / flag / key wire literals (RULE UFS) -------------------------
const CMD_SECRET = "secret" as const;
const CMD_TENANT = "tenant" as const;
const CMD_PROVIDER = "provider" as const;
const CMD_WORKSPACE = "workspace" as const;
const CMD_AGENT_KEY = "fleet-key" as const;
const CMD_LIST = "list" as const;
const SUB_CREATE = "create" as const;
const SUB_DELETE = "delete" as const;
const SUB_LIST = "list" as const;
const FLAG_DATA = "--data" as const;
const FLAG_SECRET = "--secret" as const;
const FLAG_MODEL = "--model" as const;
const FLAG_WORKSPACE_ID = "--workspace-id" as const;
const FLAG_JSON = "--json" as const;

const KEY_STATUS = "status" as const;
const KEY_DELETED = "deleted" as const;
const STATUS_STORED = "stored" as const;

const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
const ENV_NO_COLOR = "NO_COLOR" as const;
const NO_COLOR_ON = "1" as const;
const STATE_DIR_PREFIX = "agentsfleet-refint-" as const;

// A throwaway model identifier for the provider-link probe in scenario (a).
const PROBE_MODEL = "claude-sonnet-refint-probe" as const;
// Secret-payload prefix (never asserted; just a recognisable shape).
const SECRET_PAYLOAD_PREFIX = "sk-ref-" as const;

const INSTALL_TIMEOUT_MS = 120_000;
const SCENARIO_TIMEOUT_MS = 150_000;

const refName = (label: string): string => `${ACCEPTANCE_RUN_PREFIX}-${label}`;

const secretPayload = (): string =>
  JSON.stringify({ api_token: `${SECRET_PAYLOAD_PREFIX}${crypto.randomBytes(12).toString("hex")}` });

function parseJson<T>(stdout: string, label: string): T {
  const trimmed = stdout.trim();
  assert.ok(trimmed.length > 0, `${label}: empty stdout`);
  return JSON.parse(trimmed) as T;
}

interface FleetListEnvelope {
  readonly items?: ReadonlyArray<{ readonly id?: string; readonly fleet_id?: string }>;
}

if (!isLive) {
  describe("referential-integrity.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("referential-integrity — cross-resource deletes (seeded-credentials session)", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let providerBaseline: ProviderSnapshot | null = null;
    let providerMutated = false;
    let mintedKey: MintedFleetKey | null = null;

    async function run(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, stdin: "" });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      // Tenant-provider mutation is tenant-wide and likely role-gated — mint
      // the admin identity (mirrors tenant-provider-mutation.spec.ts).
      const email = resolveFixtureEmail("admin");
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

      // Snapshot tenant provider posture so scenario (a) can restore exactly.
      providerBaseline = await showProvider(env, sessionJwt);
    });

    afterAll(async () => {
      // Restore tenant provider posture EVEN ON FAILURE — shared tenant must
      // not carry this run's self_managed posture (with a since-deleted
      // secret) forward.
      if (env && sessionJwt && (providerMutated || providerBaseline)) {
        const restoreTo =
          providerBaseline ?? ({ mode: TENANT_PROVIDER_MODE.platform } as ProviderSnapshot);
        try { await restoreProviderBaseline(env, sessionJwt, restoreTo); }
        catch { /* best-effort teardown */ }
      }
      if (mintedKey) {
        try { await revokeFleetKey(env, mintedKey.fleetKeyId); }
        catch { /* best-effort key revoke */ }
      }
      if (apiUrl && sessionJwt && workspaceId) {
        try { await sweepSecrets({ apiUrl, token: sessionJwt, workspaceId }, { runPrefix: ACCEPTANCE_RUN_PREFIX }); }
        catch { /* best-effort secret sweep */ }
      }
      try { await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX }); }
      catch { /* best-effort fleet cleanup */ }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // ── (a) secret referenced by a tenant provider, then deleted ──────
    describe("(a) delete a secret a tenant provider references", () => {
      const secretName = refName("provref");

      it("create secret → provider references it → delete is refused OR cascades", async () => {
        // 1. Store the secret.
        const added = await run([CMD_SECRET, SUB_CREATE, secretName, FLAG_DATA, secretPayload(), FLAG_JSON]);
        assert.equal(added.code, 0, `secret create exited ${added.code}: ${added.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(added.stdout, "secret-create")[KEY_STATUS],
          STATUS_STORED,
          `unexpected secret create status: ${added.stdout}`,
        );

        // 2. Point the tenant provider at it. The PUT may accept the posture
        //    (possibly flagging credential_missing) or reject an unknown
        //    secret — only treat an accepted PUT as a real reference.
        const linked = await run([
          CMD_TENANT, CMD_PROVIDER, SUB_CREATE, FLAG_SECRET, secretName,
          FLAG_MODEL, PROBE_MODEL, FLAG_JSON,
        ]);
        if (linked.code === 0) {
          providerMutated = true;
          const after = await showProvider(env, sessionJwt);
          assert.equal(after.secret_ref, secretName,
            `provider did not record the secret reference: ${JSON.stringify(after)}`);
        }

        // 3. Delete the referenced secret and DISCOVER the behaviour —
        //    refused-conflict OR cascade-with-credential_missing (the helper
        //    pins the disjunction so this body stays under the fn-length bound).
        const del = await run([CMD_SECRET, SUB_DELETE, secretName, FLAG_JSON]);
        await assertSecretDeleteDisjunction({
          del,
          secretName,
          providerMutated,
          showProvider: () => showProvider(env, sessionJwt),
        });
      }, SCENARIO_TIMEOUT_MS);
    });

    // ── (b) workspace delete with a live prefix-named fleet inside ────────
    describe("(b) delete a workspace that still has a LIVE prefix-named fleet", () => {
      it("local workspace delete does not orphan the server fleet (documented behaviour)", async () => {
        // Install a live fleet into the current (bootstrap) workspace.
        const installed = await installPlatformOpsFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        const fleetId = (installed.fleet_id ?? installed.id) as string | undefined;
        assert.ok(fleetId, `install missing id: ${JSON.stringify(installed)}`);

        // `workspace delete` is a LOCAL-store op (no server DELETE route), so
        // it cannot guard against, nor cascade onto, the live fleet. The
        // documented behaviour: the local delete succeeds and the server
        // workspace + its fleet remain reachable via `list --workspace-id`.
        const del = await run([CMD_WORKSPACE, SUB_DELETE, workspaceId, FLAG_JSON]);
        assert.equal(del.code, 0, `workspace delete exited ${del.code}: ${del.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(del.stdout, "ws-del")[KEY_DELETED],
          workspaceId,
          `workspace delete echoed the wrong id: ${del.stdout}`,
        );

        // Server side is unaffected: the fleet is still listable by id even
        // though the local workspace pointer was removed. `list --workspace-id`
        // takes the explicit override without requiring the (now-deleted) local
        // store entry (per cli/src/commands/fleet_list.ts).
        const listed = await run([CMD_LIST, FLAG_WORKSPACE_ID, workspaceId, FLAG_JSON]);
        assert.equal(listed.code, 0, `list --workspace-id exited ${listed.code}: ${listed.stderr}`);
        const rows = parseJson<FleetListEnvelope>(listed.stdout, "ws-fleets").items ?? [];
        const survived = rows.some((r) => r.id === fleetId || r.fleet_id === fleetId);
        assert.ok(survived,
          `server fleet ${fleetId} vanished after a LOCAL workspace delete — ` +
          `the documented no-server-DELETE behaviour was breached: ${listed.stdout}`);

        // Re-hydrate the local store so afterAll teardown has a workspace
        // pointer again (the local delete dropped it).
        const rehydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
        workspaceId = rehydrated.currentWorkspaceId;
      }, SCENARIO_TIMEOUT_MS);
    });

    // ── (c) fleet-key secret is rejected on control-plane reads, pre+post revoke ──
    describe("(c) an agt_a… key is not a control-plane credential, before and after revoke", () => {
      let fleetId = "";

      // The control-plane read attempted with the fleet key as the bearer.
      // `fleet-key list` is workspace-scoped (same resource family the key
      // belongs to), uses the JWT-hydrated state dir, and — crucially — its
      // route is guarded by `bearer()` (JWT / agt_t only). An agt_a key is
      // rejected here both before AND after revocation.
      const KEY_AUTHED_READ: ReadonlyArray<string> = [CMD_AGENT_KEY, SUB_LIST];

      it("install a prefix-named fleet to bind the key to", async () => {
        const installed = await installPlatformOpsFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        fleetId = (installed.fleet_id ?? installed.id) as string;
        assert.ok(fleetId, `install missing id: ${JSON.stringify(installed)}`);
      }, INSTALL_TIMEOUT_MS);

      it("mint a key and capture the agt_a… secret from the add response", async () => {
        mintedKey = await mintFleetKey(env, sessionJwt, { fleetId, name: refName("authkey") });
        assert.ok(
          mintedKey.secret.startsWith(AGENT_KEY_SECRET_PREFIX),
          `minted secret is not an ${AGENT_KEY_SECRET_PREFIX}… key (shape changed?): ${mintedKey.secret.slice(0, 6)}…`,
        );
      });

      it("the agt_a… key is rejected on a control-plane read while still live (401/403)", async () => {
        assert.ok(mintedKey, "key must have been minted");
        // The key is NOT a control-plane credential: the `bearer()` middleware
        // accepts only JWT / agt_t, so this read is rejected at the auth
        // boundary even though the key was just minted and not yet revoked.
        const result = await readWithFleetKey(env, mintedKey.secret, KEY_AUTHED_READ);
        assert.notEqual(result.code, 0,
          `an agt_a key must NOT authenticate a control-plane read; got exit 0: ${result.stdout}`);
        assert.match(`${result.stdout}\n${result.stderr}`, REJECTED_AUTH_RE,
          `live agt_a control-plane read should be rejected at the auth boundary; got ${result.stderr || result.stdout}`);
      }, SCENARIO_TIMEOUT_MS);

      it("after fleet-key delete, the SAME read is still rejected (non-zero, 401/403)", async () => {
        assert.ok(mintedKey, "key must have been minted");
        const revoked = await revokeFleetKey(env, mintedKey.fleetKeyId);
        assert.equal(revoked.code, 0, `fleet-key delete exited ${revoked.code}: ${revoked.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(revoked.stdout, "key-del")[KEY_DELETED],
          true,
          `unexpected fleet-key delete envelope: ${revoked.stdout}`,
        );

        const afterRevoke = await readWithFleetKey(env, mintedKey.secret, KEY_AUTHED_READ);
        assert.notEqual(afterRevoke.code, 0,
          `a revoked fleet key must NOT authenticate; read exited 0: ${afterRevoke.stdout}`);
        assert.match(`${afterRevoke.stdout}\n${afterRevoke.stderr}`, REJECTED_AUTH_RE,
          `revoked-key read should be rejected at the auth boundary; got ${afterRevoke.stderr || afterRevoke.stdout}`);
        mintedKey = null; // already revoked — skip the afterAll re-revoke.
      }, SCENARIO_TIMEOUT_MS);
    });
  });
}
