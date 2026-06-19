/**
 * Referential-integrity / cross-resource acceptance scenarios (live,
 * AGENTSFLEET_TOKEN-injected). Mirrors lifecycle-with-token / credential-vault
 * / agent-key-mutation: mint a Clerk session JWT, hydrate workspaces.json from
 * the API, then walk cross-resource deletes whose outcome the suite DISCOVERS
 * and DOCUMENTS against api-dev rather than presumes. The load-bearing contract
 * for each scenario is restated as an inline comment at its `describe` block:
 *
 *   (a) delete a credential a tenant provider references → refused-conflict OR
 *       cascade-with-credential_missing disjunction; baseline restored on fail.
 *   (b) `workspace delete` is LOCAL-only (no server DELETE) → the server agent
 *       survives and stays reachable via `list --workspace-id`.
 *   (c) an `agt_a…` agent key is NOT a control-plane credential — it is rejected
 *       on a control-plane read (`agent-key list`) both before AND after revoke.
 *
 * Prefix-scoped: every agent + credential + key is ACCEPTANCE_RUN_PREFIX-named
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
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import { sweepCredentials } from "./fixtures/credential-ops.ts";
import {
  TENANT_PROVIDER_MODE,
  restoreProviderBaseline,
  showProvider,
} from "./fixtures/tenant-provider-ops.ts";
import type { ProviderSnapshot } from "./fixtures/tenant-provider-ops.ts";
import {
  AGENT_KEY_SECRET_PREFIX,
  REJECTED_AUTH_RE,
  assertCredentialDeleteDisjunction,
  mintAgentKey,
  readWithAgentKey,
  revokeAgentKey,
} from "./fixtures/referential-ops.ts";
import type { MintedAgentKey } from "./fixtures/referential-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// --- command / flag / key wire literals (RULE UFS) -------------------------
const CMD_CREDENTIAL = "credential" as const;
const CMD_TENANT = "tenant" as const;
const CMD_PROVIDER = "provider" as const;
const CMD_WORKSPACE = "workspace" as const;
const CMD_AGENT_KEY = "agent-key" as const;
const CMD_LIST = "list" as const;
const SUB_ADD = "add" as const;
const SUB_DELETE = "delete" as const;
const SUB_LIST = "list" as const;
const FLAG_DATA = "--data" as const;
const FLAG_CREDENTIAL = "--credential" as const;
const FLAG_MODEL = "--model" as const;
const FLAG_WORKSPACE_ID = "--workspace-id" as const;
const FLAG_JSON = "--json" as const;

const KEY_STATUS = "status" as const;
const KEY_DELETED = "deleted" as const;
const STATUS_STORED = "stored" as const;

const ENV_TOKEN = "AGENTSFLEET_TOKEN" as const;
const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
const ENV_NO_COLOR = "NO_COLOR" as const;
const NO_COLOR_ON = "1" as const;
const STATE_DIR_PREFIX = "agentsfleet-refint-" as const;

// A throwaway model identifier for the provider-link probe in scenario (a).
const PROBE_MODEL = "claude-sonnet-refint-probe" as const;
// Credential-payload secret prefix (never asserted; just a recognisable shape).
const CREDENTIAL_SECRET_PREFIX = "sk-ref-" as const;

const INSTALL_TIMEOUT_MS = 120_000;
const SCENARIO_TIMEOUT_MS = 150_000;

const refName = (label: string): string => `${ACCEPTANCE_RUN_PREFIX}-${label}`;

const credentialPayload = (): string =>
  JSON.stringify({ api_token: `${CREDENTIAL_SECRET_PREFIX}${crypto.randomBytes(12).toString("hex")}` });

function parseJson<T>(stdout: string, label: string): T {
  const trimmed = stdout.trim();
  assert.ok(trimmed.length > 0, `${label}: empty stdout`);
  return JSON.parse(trimmed) as T;
}

interface AgentListEnvelope {
  readonly items?: ReadonlyArray<{ readonly id?: string; readonly agent_id?: string }>;
}

if (!isLive) {
  describe("referential-integrity.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("referential-integrity — cross-resource deletes (token injection)", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let providerBaseline: ProviderSnapshot | null = null;
    let providerMutated = false;
    let mintedKey: MintedAgentKey | null = null;

    async function run(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runAgentctl(args, { env, stdin: "" });
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
        [ENV_TOKEN]: sessionJwt,
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
      // credential) forward.
      if (env && sessionJwt && (providerMutated || providerBaseline)) {
        const restoreTo =
          providerBaseline ?? ({ mode: TENANT_PROVIDER_MODE.platform } as ProviderSnapshot);
        try { await restoreProviderBaseline(env, sessionJwt, restoreTo); }
        catch { /* best-effort teardown */ }
      }
      if (mintedKey) {
        try { await revokeAgentKey(env, mintedKey.agentKeyId); }
        catch { /* best-effort key revoke */ }
      }
      if (apiUrl && sessionJwt && workspaceId) {
        try { await sweepCredentials({ apiUrl, token: sessionJwt, workspaceId }, { runPrefix: ACCEPTANCE_RUN_PREFIX }); }
        catch { /* best-effort credential sweep */ }
      }
      try { await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX }); }
      catch { /* best-effort agent cleanup */ }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // ── (a) credential referenced by a tenant provider, then deleted ──────
    describe("(a) delete a credential a tenant provider references", () => {
      const credName = refName("provref");

      it("add credential → provider references it → delete is refused OR cascades", async () => {
        // 1. Store the credential.
        const added = await run([CMD_CREDENTIAL, SUB_ADD, credName, FLAG_DATA, credentialPayload(), FLAG_JSON]);
        assert.equal(added.code, 0, `credential add exited ${added.code}: ${added.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(added.stdout, "cred-add")[KEY_STATUS],
          STATUS_STORED,
          `unexpected credential add status: ${added.stdout}`,
        );

        // 2. Point the tenant provider at it. The PUT may accept the posture
        //    (possibly flagging credential_missing) or reject an unknown
        //    credential — only treat an accepted PUT as a real reference.
        const linked = await run([
          CMD_TENANT, CMD_PROVIDER, SUB_ADD, FLAG_CREDENTIAL, credName,
          FLAG_MODEL, PROBE_MODEL, FLAG_JSON,
        ]);
        if (linked.code === 0) {
          providerMutated = true;
          const after = await showProvider(env, sessionJwt);
          assert.equal(after.credential_ref, credName,
            `provider did not record the credential reference: ${JSON.stringify(after)}`);
        }

        // 3. Delete the referenced credential and DISCOVER the behaviour —
        //    refused-conflict OR cascade-with-credential_missing (the helper
        //    pins the disjunction so this body stays under the fn-length bound).
        const del = await run([CMD_CREDENTIAL, SUB_DELETE, credName, FLAG_JSON]);
        await assertCredentialDeleteDisjunction({
          del,
          credName,
          providerMutated,
          showProvider: () => showProvider(env, sessionJwt),
        });
      }, SCENARIO_TIMEOUT_MS);
    });

    // ── (b) workspace delete with a live prefix-named agent inside ────────
    describe("(b) delete a workspace that still has a LIVE prefix-named agent", () => {
      it("local workspace delete does not orphan the server agent (documented behaviour)", async () => {
        // Install a live agent into the current (bootstrap) workspace.
        const installed = await installPlatformOpsAgent({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        const agentId = (installed.agent_id ?? installed.id) as string | undefined;
        assert.ok(agentId, `install missing id: ${JSON.stringify(installed)}`);

        // `workspace delete` is a LOCAL-store op (no server DELETE route), so
        // it cannot guard against, nor cascade onto, the live agent. The
        // documented behaviour: the local delete succeeds and the server
        // workspace + its agent remain reachable via `list --workspace-id`.
        const del = await run([CMD_WORKSPACE, SUB_DELETE, workspaceId, FLAG_JSON]);
        assert.equal(del.code, 0, `workspace delete exited ${del.code}: ${del.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(del.stdout, "ws-del")[KEY_DELETED],
          workspaceId,
          `workspace delete echoed the wrong id: ${del.stdout}`,
        );

        // Server side is unaffected: the agent is still listable by id even
        // though the local workspace pointer was removed. `list --workspace-id`
        // takes the explicit override without requiring the (now-deleted) local
        // store entry (per cli/src/commands/agent_list.ts).
        const listed = await run([CMD_LIST, FLAG_WORKSPACE_ID, workspaceId, FLAG_JSON]);
        assert.equal(listed.code, 0, `list --workspace-id exited ${listed.code}: ${listed.stderr}`);
        const rows = parseJson<AgentListEnvelope>(listed.stdout, "ws-agents").items ?? [];
        const survived = rows.some((r) => r.id === agentId || r.agent_id === agentId);
        assert.ok(survived,
          `server agent ${agentId} vanished after a LOCAL workspace delete — ` +
          `the documented no-server-DELETE behaviour was breached: ${listed.stdout}`);

        // Re-hydrate the local store so afterAll teardown has a workspace
        // pointer again (the local delete dropped it).
        const rehydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
        workspaceId = rehydrated.currentWorkspaceId;
      }, SCENARIO_TIMEOUT_MS);
    });

    // ── (c) agent-key secret is rejected on control-plane reads, pre+post revoke ──
    describe("(c) an agt_a… key is not a control-plane credential, before and after revoke", () => {
      let agentId = "";

      // The control-plane read attempted with the agent key as the bearer.
      // `agent-key list` is workspace-scoped (same resource family the key
      // belongs to), uses the JWT-hydrated state dir, and — crucially — its
      // route is guarded by `bearer()` (JWT / agt_t only). An agt_a key is
      // rejected here both before AND after revocation.
      const KEY_AUTHED_READ: ReadonlyArray<string> = [CMD_AGENT_KEY, SUB_LIST];

      it("install a prefix-named agent to bind the key to", async () => {
        const installed = await installPlatformOpsAgent({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        agentId = (installed.agent_id ?? installed.id) as string;
        assert.ok(agentId, `install missing id: ${JSON.stringify(installed)}`);
      }, INSTALL_TIMEOUT_MS);

      it("mint a key and capture the agt_a… secret from the add response", async () => {
        mintedKey = await mintAgentKey(env, sessionJwt, { agentId, name: refName("authkey") });
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
        const result = await readWithAgentKey(env, mintedKey.secret, KEY_AUTHED_READ);
        assert.notEqual(result.code, 0,
          `an agt_a key must NOT authenticate a control-plane read; got exit 0: ${result.stdout}`);
        assert.match(`${result.stdout}\n${result.stderr}`, REJECTED_AUTH_RE,
          `live agt_a control-plane read should be rejected at the auth boundary; got ${result.stderr || result.stdout}`);
      }, SCENARIO_TIMEOUT_MS);

      it("after agent-key delete, the SAME read is still rejected (non-zero, 401/403)", async () => {
        assert.ok(mintedKey, "key must have been minted");
        const revoked = await revokeAgentKey(env, mintedKey.agentKeyId);
        assert.equal(revoked.code, 0, `agent-key delete exited ${revoked.code}: ${revoked.stderr}`);
        assert.equal(
          parseJson<Record<string, unknown>>(revoked.stdout, "key-del")[KEY_DELETED],
          true,
          `unexpected agent-key delete envelope: ${revoked.stdout}`,
        );

        const afterRevoke = await readWithAgentKey(env, mintedKey.secret, KEY_AUTHED_READ);
        assert.notEqual(afterRevoke.code, 0,
          `a revoked agent key must NOT authenticate; read exited 0: ${afterRevoke.stdout}`);
        assert.match(`${afterRevoke.stdout}\n${afterRevoke.stderr}`, REJECTED_AUTH_RE,
          `revoked-key read should be rejected at the auth boundary; got ${afterRevoke.stderr || afterRevoke.stdout}`);
        mintedKey = null; // already revoked — skip the afterAll re-revoke.
      }, SCENARIO_TIMEOUT_MS);
    });
  });
}
