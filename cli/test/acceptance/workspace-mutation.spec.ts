/**
 * Workspace-mutation acceptance scenario (live, seeded-credentials session).
 *
 * Mints a Clerk session JWT via the admin path, hydrates workspaces.json
 * directly from the API (same bootstrap as lifecycle-with-token.spec.ts —
 * the CLI only populates that file inside the login flow), then exercises
 * the workspace mutating surface confirmed against
 * `src/program/cli-tree.ts` + `src/commands/workspace.ts`:
 *
 *   1. round-trip — `workspace create` → `workspace list` contains it →
 *      `workspace use` → `workspace show` reflects active →
 *      `workspace delete` (LOCAL store removal) → list excludes it.
 *   2. scope isolation — two created workspaces; a prefix-named fleet
 *      installed into WS-A appears under WS-A's scope and is ABSENT under
 *      WS-B's scope (server enforces workspace-scoped fleet reads). The
 *      presence/absence verdict keys off the install envelope's
 *      `fleet_id` (always emitted by `fleet install --json`), not the
 *      display name — id is the stronger invariant.
 *
 * The minted JWT must never appear in stdout/stderr — `assertNoSecretLeak`
 * fires after every spawn that runs in the seeded-credentials session.
 *
 * HARD SERVER CONSTRAINT: `workspace delete` removes the entry from the
 * LOCAL store only — `src/commands/workspace.ts` issues no DELETE and the
 * server-side workspace persists. So every created workspace is permanent
 * residue in the shared DEV tenant; names are `ACCEPTANCE_RUN_PREFIX`-scoped
 * to keep that residue attributable, and the fleets installed inside are
 * torn down via `cleanWorkspaceFleets`.
 *
 * Teardown caveat: `cleanWorkspaceFleets` lists the *current* workspace and
 * filters by `workspace_id`, so this suite switches the active workspace to
 * each created workspace (`workspace use`) BEFORE cleaning it — otherwise the
 * fleet never appears in the listing and the kill is skipped, leaking a live
 * fleet into the shared tenant.
 *
 * Live-only: the suite registers only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL; otherwise every test is skipped (matches the unit runner's
 * local invariant — CI runs these live).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, TERMINAL_STATUSES } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import {
  WS_ID_KEY,
  WS_SHOW_ACTIVE_KEY,
  WORKSPACE_LOCAL_REMOVAL_FIELD,
  AGENT_ID_KEY,
  AGENT_NAME_KEY,
  FLAG_JSON,
  addWorkspace,
  listWorkspaces,
  useWorkspace,
  listFleetsIn,
  fleetIdOf,
  hasFleetWithId,
} from "./fixtures/workspace-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

const WS_A_NAME = `${ACCEPTANCE_RUN_PREFIX}-ws-a`;
const WS_B_NAME = `${ACCEPTANCE_RUN_PREFIX}-ws-b`;
const RT_NAME = `${ACCEPTANCE_RUN_PREFIX}-ws-rt`;
const INSTALL_TIMEOUT_MS = 120_000;
const SCOPE_TIMEOUT_MS = 150_000;

if (!isLive) {
  describe("workspace-mutation.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("workspace-mutation — seeded-credentials session", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let bootstrapWorkspaceId = "";
    const createdWorkspaceIds = new Set<string>();

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-ws-mutation-"));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      bootstrapWorkspaceId = hydrated.currentWorkspaceId;
    });

    afterAll(async () => {
      // Kill this run's fleets in every workspace we created. The teardown
      // helper lists the CURRENT workspace, so switch to each created one
      // first — otherwise its fleets never appear in the listing and the
      // kill is skipped (live residue in the shared DEV tenant). Best-effort:
      // a failed kill must not mask the test verdict.
      for (const wsId of createdWorkspaceIds) {
        try {
          await useWorkspace(env, wsId);
          await cleanWorkspaceFleets(env, { workspaceId: wsId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // ── Scenario 1: workspace mutating round-trip ────────────────────
    describe("round-trip add → list → use → show → delete", () => {
      let createdId = "";

      it("workspace create creates a server workspace and returns its id", async () => {
        const added = await addWorkspace(env, RT_NAME);
        createdId = added.workspaceId;
        createdWorkspaceIds.add(createdId);
        assert.notEqual(createdId, bootstrapWorkspaceId,
          "new workspace id must differ from the bootstrap workspace");
      });

      it("workspace list --json contains the created workspace", async () => {
        const rows = await listWorkspaces(env);
        const mine = rows.find((row) => row[WS_ID_KEY] === createdId);
        assert.ok(mine, `created workspace ${createdId} missing from list: ${JSON.stringify(rows)}`);
      });

      it("workspace use makes it the active workspace", async () => {
        await useWorkspace(env, createdId);
      });

      it("workspace show --json reflects the active workspace", async () => {
        const result = await runWithEnv(["workspace", "show", FLAG_JSON]);
        assert.equal(result.code, 0, `workspace show exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
        assert.equal(parsed[WS_ID_KEY], createdId,
          `workspace show ${WS_ID_KEY} mismatch: ${result.stdout}`);
        assert.equal(parsed[WS_SHOW_ACTIVE_KEY], true,
          `workspace show should report active=true for the used workspace: ${result.stdout}`);
      });

      it("workspace delete removes it from the local store", async () => {
        // Re-point the active workspace away first: deleting the current
        // workspace re-homes `current_workspace_id` to the next entry,
        // which would leave the run targeting an unintended workspace.
        await useWorkspace(env, bootstrapWorkspaceId);
        const result = await runWithEnv(["workspace", "delete", createdId, FLAG_JSON]);
        assert.equal(result.code, 0, `workspace delete exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
        assert.equal(parsed[WORKSPACE_LOCAL_REMOVAL_FIELD], createdId,
          `workspace delete ${WORKSPACE_LOCAL_REMOVAL_FIELD} mismatch: ${result.stdout}`);
        // The server workspace persists (no DELETE route); drop it from the
        // teardown set only because no fleet was ever installed inside it.
        createdWorkspaceIds.delete(createdId);
      });

      it("workspace list --json excludes the deleted workspace", async () => {
        const rows = await listWorkspaces(env);
        const stillThere = rows.find((row) => row[WS_ID_KEY] === createdId);
        assert.equal(stillThere, undefined,
          `deleted workspace ${createdId} still present in list: ${JSON.stringify(rows)}`);
      });
    });

    // ── Scenario 2: cross-workspace scope isolation ──────────────────
    describe("scope isolation across two workspaces", () => {
      let wsA = "";
      let wsB = "";
      let fleetId = "";

      it("create two distinct workspaces", async () => {
        const a = await addWorkspace(env, WS_A_NAME);
        const b = await addWorkspace(env, WS_B_NAME);
        wsA = a.workspaceId;
        wsB = b.workspaceId;
        createdWorkspaceIds.add(wsA);
        createdWorkspaceIds.add(wsB);
        assert.notEqual(wsA, wsB, "the two created workspaces must have distinct ids");
      });

      it("install a prefix-named fleet into WS-A", async () => {
        // install targets the *current* workspace (no --workspace flag on
        // the install command), so select WS-A first.
        await useWorkspace(env, wsA);
        const installed = await installPlatformOpsFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        fleetId = fleetIdOf(installed);
        const installedName = installed[AGENT_NAME_KEY];
        assert.ok(
          typeof installedName === "string" && installedName.startsWith(ACCEPTANCE_RUN_PREFIX),
          `installed fleet name must carry the run prefix: ${JSON.stringify(installed)}`,
        );
      }, INSTALL_TIMEOUT_MS);

      it("the fleet appears under WS-A scope", async () => {
        const rows = await listFleetsIn(env, wsA);
        assert.ok(hasFleetWithId(rows, fleetId),
          `expected fleet ${fleetId} in WS-A (${wsA}); got: ` +
          `${JSON.stringify(rows.map((r) => r[AGENT_ID_KEY] ?? r.id))}`);
      });

      it("the fleet is ABSENT under WS-B scope", async () => {
        const rows = await listFleetsIn(env, wsB);
        assert.ok(!hasFleetWithId(rows, fleetId),
          `fleet ${fleetId} leaked into WS-B (${wsB}) — workspace scoping breached: ` +
          `${JSON.stringify(rows.map((r) => r[AGENT_ID_KEY] ?? r.id))}`);
      });

      it("teardown leaves no LIVE run-prefixed fleet in WS-A", async () => {
        await useWorkspace(env, wsA);
        await cleanWorkspaceFleets(env, { workspaceId: wsA, runPrefix: ACCEPTANCE_RUN_PREFIX });
        const rows = await listFleetsIn(env, wsA);
        const mineLive = rows.filter((row) =>
          typeof row.name === "string" &&
          row.name.startsWith(ACCEPTANCE_RUN_PREFIX) &&
          !TERMINAL_STATUSES.includes(row.status ?? ""),
        );
        assert.equal(mineLive.length, 0,
          `expected zero live run-prefixed fleets in WS-A; got: ${JSON.stringify(mineLive)}`);
      }, SCOPE_TIMEOUT_MS);
    });
  });
}
