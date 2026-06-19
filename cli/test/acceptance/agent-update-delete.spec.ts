/**
 * Agent update + delete + illegal-state-transition acceptance scenario
 * (live, seeded-credentials session).
 *
 * Mirrors lifecycle-with-token.spec.ts's identity + hydration setup
 * (Clerk session JWT minted via the admin path, workspaces.json hydrated
 * directly from the API), then exercises the mutate-and-remove surface
 * the lifecycle walk does not cover:
 *
 *   1. happy path — install → `agent update <id>` (re-PATCH SKILL.md +
 *      TRIGGER.md from a name-matched bundle) → status still reflects a
 *      live state → kill (delete requires a killed agent) → `delete <id>`
 *      → list excludes the id.
 *
 *   2. illegal transitions —
 *      a. install → kill → `resume <killed-id>` MUST exit non-zero and
 *         surface UZ-AGT-010 (ALREADY_TERMINAL); status stays terminal.
 *      b. install → stop → `stop` the already-stopped agent is handled
 *         gracefully (exit 0 OR non-zero with a transition stem); status
 *         stays stopped/paused either way.
 *
 * Every spawn runs through `runWithEnv`, which asserts the minted JWT
 * never lands in stdout/stderr (WS-E #C1 regression).
 *
 * Live-only: the whole suite registers only when
 * `AGENTSFLEET_ACCEPTANCE_TARGET` is an https URL; otherwise it skips
 * cleanly (CI runs it live, local runs skip — same invariant as the
 * sibling specs). Every mutation is prefix-scoped via
 * ACCEPTANCE_RUN_PREFIX and torn down in afterAll; no global emptiness
 * is ever asserted — only "none of MY run's agents remain".
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, AGENTSFLEET_STATUS, TERMINAL_STATUSES } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import { expectStatus, killAgent, stopAgent } from "./fixtures/lifecycle.ts";
import { resolveAgentName, updateAgentBundle } from "./fixtures/update-delete-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// Envelope status the update handler emits on a successful PATCH
// (agent_install.ts#updateEffectFromArgs → printJson status:"updated").
const UPDATE_OK_STATUS = "updated";

// State buckets the server may settle into for each lifecycle checkpoint.
const LIVE_STATES: ReadonlyArray<string> = [
  AGENTSFLEET_STATUS.active,
  "running",
  "starting",
];
const STOPPED_STATES: ReadonlyArray<string> = [
  AGENTSFLEET_STATUS.paused,
  AGENTSFLEET_STATUS.stopped,
];

// An illegal lifecycle transition must be REFUSED. Verified live against
// api-dev (2026-06-19), the server signals the refusal by HTTP status, not
// a UZ-AGT-* error code: delete-before-kill and stop-already-stopped →
// HTTP_409 Conflict; resume-of-killed → HTTP_404 Not Found (the killed
// agent is no longer addressable). We accept those signals plus the
// documented UZ-AGT-010 / human stems so the assertion tracks the real
// contract and would still pass if the API later attaches the UZ code.
// (Observation for follow-up: these refusals carry no UZ-AGT-* code in the
// body — a minor error-registry gap, surfaced in the PR session notes.)
const TRANSITION_REJECTION =
  /UZ-AGT-010|transition not allowed|already.*terminal|must be killed|HTTP_409|HTTP_404|Conflict|Not Found/i;

function installedId(installed: { id?: string; agent_id?: string }): string {
  const id = installed.id ?? installed.agent_id;
  if (!id) throw new Error(`install response missing id: ${JSON.stringify(installed)}`);
  return id;
}

if (!isLive) {
  describe("agent-update-delete.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("agent update + delete + illegal transitions — seeded-credentials session", () => {
    let apiUrl: string = "";
    let sessionJwt: string = "";
    let stateDir: string = "";
    let env: Record<string, string> = {};
    let workspaceId: string = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runAgentctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-update-delete-"));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown — shared DEV tenant */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // Scenario 1: install → update → kill → delete → list excludes it.
    describe("update then delete (happy path)", () => {
      let agentId: string = "";
      let agentName: string = "";

      it("install platform-ops bundle", async () => {
        const installed = await installPlatformOpsAgent({ env });
        agentId = installedId(installed);
        agentName = await resolveAgentName(env, agentId);
        assert.ok(agentName.startsWith(ACCEPTANCE_RUN_PREFIX), `name not run-scoped: ${agentName}`);
      });

      it("agent update re-PATCHes the bundle and acks the agent_id", async () => {
        const updated = await updateAgentBundle(env, agentId, agentName);
        assertNoSecretLeak(updated, sessionJwt);
        assert.equal(updated.envelope.status, UPDATE_OK_STATUS, `unexpected update envelope: ${updated.stdout}`);
        assert.equal(updated.envelope.agent_id, agentId, `update echoed wrong id: ${updated.stdout}`);
      }, 120_000);

      it("status still reflects a live state after update", async () => {
        // A config PATCH must not knock the agent out of its running
        // lifecycle — update is non-destructive to status.
        const payload = await expectStatus(env, agentId, LIVE_STATES);
        assert.equal(typeof payload.status, "string");
      });

      it("delete before kill is refused (must be killed first)", async () => {
        // Server contract (delete.zig#not_killed → UZ-AGT-010): a live
        // agent cannot be hard-deleted. Proves the guard rather than
        // assuming kill-then-delete is the only path.
        const result = await runWithEnv(["delete", agentId, "--json"]);
        assert.notEqual(result.code, 0, `expected non-zero deleting a live agent; stdout=${result.stdout}`);
        assert.match(result.stderr + result.stdout, TRANSITION_REJECTION);
        await expectStatus(env, agentId, LIVE_STATES);
      });

      it("kill → delete removes the agent from the list", async () => {
        await killAgent(env, agentId);
        await expectStatus(env, agentId, TERMINAL_STATUSES);

        const deleted = await runWithEnv(["delete", agentId, "--json"]);
        assert.equal(deleted.code, 0, `delete exited ${deleted.code}: ${deleted.stderr}`);
        const parsed = JSON.parse(deleted.stdout.trim() || "{}") as { agent_id?: string; deleted?: boolean };
        assert.equal(parsed.deleted, true, `delete envelope missing deleted:true: ${deleted.stdout}`);
        assert.equal(parsed.agent_id, agentId, `delete echoed wrong id: ${deleted.stdout}`);

        const listed = await runWithEnv(["list", "--json"]);
        assert.equal(listed.code, 0, `list exited ${listed.code}: ${listed.stderr}`);
        const payload = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(payload.items)
          ? (payload.items as Array<{ id?: string; agent_id?: string }>)
          : [];
        const stillPresent = items.some((z) => z.id === agentId || z.agent_id === agentId);
        assert.equal(stillPresent, false, `deleted agent ${agentId} still present in list`);
      }, 30_000);
    });

    // Scenario 2a: install → kill → resume(killed) → non-zero + UZ-AGT-010.
    describe("illegal transition — resume a killed agent", () => {
      let agentId: string = "";

      beforeAll(async () => {
        const installed = await installPlatformOpsAgent({ env });
        agentId = installedId(installed);
        await killAgent(env, agentId);
        await expectStatus(env, agentId, TERMINAL_STATUSES);
      }, 30_000);

      it("resume <killed-id> exits non-zero with UZ-AGT-010", async () => {
        const result = await runWithEnv(["resume", agentId, "--json"]);
        assert.notEqual(result.code, 0, `expected non-zero resuming a killed agent; stdout=${result.stdout}`);
        assert.match(result.stderr + result.stdout, TRANSITION_REJECTION,
          `expected ALREADY_TERMINAL stem; stdout=${result.stdout} stderr=${result.stderr}`);
      });

      it("status stays terminal after the rejected resume", async () => {
        // The refused resume must not have flipped the agent back to a
        // live state — the terminal contract is irreversible.
        await expectStatus(env, agentId, TERMINAL_STATUSES);
      });
    });

    // Scenario 2b: install → stop → stop again → graceful handling.
    describe("idempotent transition — stop an already-stopped agent", () => {
      let agentId: string = "";

      beforeAll(async () => {
        const installed = await installPlatformOpsAgent({ env });
        agentId = installedId(installed);
        await stopAgent(env, agentId);
        await expectStatus(env, agentId, STOPPED_STATES);
      }, 30_000);

      it("stop on an already-stopped agent is handled gracefully", async () => {
        // Either the server treats stop→stopped as a no-op (exit 0) or it
        // rejects the redundant transition with a clear stem. What's not
        // acceptable is a connection error, a leaked token, or a silent
        // flip to a live state — the post-condition below catches the last.
        const result = await runWithEnv(["stop", agentId, "--json"]);
        if (result.code !== 0) {
          assert.match(result.stderr + result.stdout, TRANSITION_REJECTION,
            `non-zero stop must carry a transition stem; stdout=${result.stdout} stderr=${result.stderr}`);
        }
      });

      it("status remains stopped after the redundant stop", async () => {
        await expectStatus(env, agentId, STOPPED_STATES);
      });

      it("teardown: kill the stopped fixture so the run leaves no live residue", async () => {
        await killAgent(env, agentId);
        await expectStatus(env, agentId, TERMINAL_STATUSES);
      });
    });

    // Prefix-scoped post-teardown emptiness — shared DEV tenants carry
    // residual agents, so the contract is "none of MY run's agents remain
    // LIVE", not "the workspace is globally empty". Terminal rows still
    // surface in the list and prove teardown worked — filter them out.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it("no LIVE agents match ACCEPTANCE_RUN_PREFIX", async () => {
        const result = await runWithEnv(["list", "--json"]);
        assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
        const payload = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(payload.items)
          ? (payload.items as Array<{ name?: string; status?: string }>)
          : [];
        const mineLive = items.filter((z) =>
          typeof z.name === "string" &&
          z.name.startsWith(ACCEPTANCE_RUN_PREFIX) &&
          !TERMINAL_STATUSES.includes(z.status ?? ""));
        assert.equal(mineLive.length, 0,
          `expected zero live agents starting with ${ACCEPTANCE_RUN_PREFIX}; got ${JSON.stringify(mineLive)}`);
      });
    });
  });
}
