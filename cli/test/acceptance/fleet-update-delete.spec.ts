// Reviewed setup revision, lifecycle and purge behavior through the terminal.

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, AGENTSFLEET_STATUS, TERMINAL_STATUSES } from "./fixtures/constants.ts";
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
import { saveProbeFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import {
  archiveFleet,
  expectStatus,
  TOMBSTONE_REFUSAL,
} from "./fixtures/lifecycle.ts";
import { resolveFleetName, applyReviewedImprovement } from "./fixtures/improvement-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// State buckets the server may settle into for each lifecycle checkpoint.
const LIVE_STATES: ReadonlyArray<string> = [
  AGENTSFLEET_STATUS.active,
  "running",
  "starting",
];

const INSTALL_TIMEOUT_MS = 90_000;
const SETUP_TIMEOUT_MS = 120_000;

function installedId(installed: { id?: string; fleet_id?: string }): string {
  const id = installed.id ?? installed.fleet_id;
  if (!id) throw new Error(`install response missing id: ${JSON.stringify(installed)}`);
  return id;
}

if (!isLive) {
  describe("fleet-update-delete.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("fleet update + delete + illegal transitions — seeded-credentials session", () => {
    let apiUrl: string = "";
    let sessionJwt: string = "";
    let stateDir: string = "";
    let env: Record<string, string> = {};
    let workspaceId: string = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env });
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
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown — shared DEV tenant */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // Scenario 1: install → update → kill → delete → list excludes it.
    describe("update then delete (happy path)", () => {
      let fleetId: string = "";
      let fleetName: string = "";

      it("Save reviewed native setup", async () => {
        const installed = await saveProbeFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        fleetId = installedId(installed);
        fleetName = await resolveFleetName(env, fleetId);
        assert.ok(fleetName.startsWith(ACCEPTANCE_RUN_PREFIX), `name not run-scoped: ${fleetName}`);
      }, SETUP_TIMEOUT_MS);

      it("Apply advances one reviewed setup revision and identifies its fleet", async () => {
        const updated = await applyReviewedImprovement(env, fleetId);
        assertNoSecretLeak(updated, sessionJwt);
        assert.equal(BigInt(updated.envelope.setup_revision), BigInt(updated.previousRevision) + 1n);
        assert.equal(updated.envelope.fleet_id, fleetId, `update echoed wrong id: ${updated.stdout}`);
      }, 120_000);

      it("status still reflects a live state after update", async () => {
        // A config PATCH must not knock the fleet out of its running
        // lifecycle — update is non-destructive to status.
        const payload = await expectStatus(env, fleetId, LIVE_STATES);
        assert.equal(typeof payload.status, "string");
      });

      it("delete removes a live fleet from the list", async () => {
        const deleted = await runWithEnv(["fleet", "delete", fleetId, "--json"]);
        assert.equal(deleted.code, 0, `delete exited ${deleted.code}: ${deleted.stderr}`);
        const parsed = JSON.parse(deleted.stdout.trim() || "{}") as { fleet_id?: string; deleted?: boolean };
        assert.equal(parsed.deleted, true, `delete envelope missing deleted:true: ${deleted.stdout}`);
        assert.equal(parsed.fleet_id, fleetId, `delete echoed wrong id: ${deleted.stdout}`);

        const listed = await runWithEnv(["fleet", "list", "--json"]);
        assert.equal(listed.code, 0, `list exited ${listed.code}: ${listed.stderr}`);
        const payload = JSON.parse(listed.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(payload.items)
          ? (payload.items as Array<{ id?: string; fleet_id?: string }>)
          : [];
        const stillPresent = items.some((z) => z.id === fleetId || z.fleet_id === fleetId);
        assert.equal(stillPresent, false, `deleted fleet ${fleetId} still present in list`);
      }, 30_000);
    });

    // Scenario 2: install → archive → archive again → refused as a tombstone.
    describe("archive is terminal", () => {
      let fleetId: string = "";

      beforeAll(async () => {
        const installed = await saveProbeFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        fleetId = installedId(installed);
        await archiveFleet(env, fleetId);
        await expectStatus(env, fleetId, TERMINAL_STATUSES);
      }, SETUP_TIMEOUT_MS);

      it("archive <archived-id> is refused — an archived fleet never changes again", async () => {
        const result = await runWithEnv(["fleet", "archive", fleetId, "--json"]);
        assert.notEqual(result.code, 0, `expected non-zero archiving an archived fleet; stdout=${result.stdout}`);
        assert.match(result.stderr + result.stdout, TOMBSTONE_REFUSAL,
          `an archived fleet is a tombstone; stdout=${result.stdout} stderr=${result.stderr}`);
        await expectStatus(env, fleetId, TERMINAL_STATUSES);
      });

      it("teardown: delete the archived fixture", async () => {
        const result = await runWithEnv(["fleet", "delete", fleetId, "--json"]);
        assert.equal(result.code, 0, `delete exited ${result.code}: ${result.stderr}`);
      });
    });

    // Prefix-scoped post-teardown emptiness — shared DEV tenants carry
    // residual fleets, so the contract is "none of MY run's fleets remain
    // LIVE", not "the workspace is globally empty". Terminal rows still
    // surface in the list and prove teardown worked — filter them out.
    describe("post-teardown emptiness (prefix-scoped)", () => {
      beforeAll(async () => {
        await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
      });

      it("no LIVE fleets match ACCEPTANCE_RUN_PREFIX", async () => {
        const result = await runWithEnv(["fleet", "list", "--json"]);
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
          `expected zero live fleets starting with ${ACCEPTANCE_RUN_PREFIX}; got ${JSON.stringify(mineLive)}`);
      });
    });
  });
}
