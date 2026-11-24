// A fleet's model and network changes, lifecycle and purge through the terminal.

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
import { createProbeFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import {
  archiveFleet,
  expectStatus,
  getStatus,
  ARCHIVED_REFUSAL,
} from "./fixtures/lifecycle.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

const INSTALL_TIMEOUT_MS = 90_000;
const SETUP_TIMEOUT_MS = 120_000;

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

    // Scenario 1: create → change model and network → delete → list excludes it.
    describe("change then delete (happy path)", () => {
      let fleetId: string = "";

      it("fleet create starts a run-scoped fleet", async () => {
        const created = await createProbeFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS });
        fleetId = created.id;
        assert.ok(created.name.startsWith(ACCEPTANCE_RUN_PREFIX), `name not run-scoped: ${created.name}`);
      }, SETUP_TIMEOUT_MS);

      it("fleet model set answers the fleet on the model it names", async () => {
        const before = await getStatus(env, fleetId);
        const result = await runWithEnv(["fleet", "model", "set", fleetId, before.model_ref, "--json"]);
        assert.equal(result.code, 0, `model set exited ${result.code}: ${result.stderr}`);
        const fleet = JSON.parse(result.stdout) as { id?: string; model_ref?: string };
        assert.equal(fleet.id, fleetId, `model set answered another fleet: ${result.stdout}`);
        assert.equal(fleet.model_ref, before.model_ref);
      }, 30_000);

      it("fleet network set --private --no-packages closes the network", async () => {
        const result = await runWithEnv(["fleet", "network", "set", fleetId, "--private", "--no-packages", "--json"]);
        assert.equal(result.code, 0, `network set exited ${result.code}: ${result.stderr}`);
        const fleet = JSON.parse(result.stdout) as { http_egress?: string; package_domains?: boolean };
        assert.equal(fleet.http_egress, "restricted");
        assert.equal(fleet.package_domains, false);
      }, 30_000);

      it("a changed fleet stays active", async () => {
        await expectStatus(env, fleetId, AGENTSFLEET_STATUS.active);
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

    // Scenario 2: create → archive → archive again → refused as archived (409).
    describe("archive is terminal", () => {
      let fleetId: string = "";

      beforeAll(async () => {
        fleetId = (await createProbeFleet({ env, timeoutMs: INSTALL_TIMEOUT_MS })).id;
        await archiveFleet(env, fleetId);
        await expectStatus(env, fleetId, TERMINAL_STATUSES);
      }, SETUP_TIMEOUT_MS);

      it("archive <archived-id> is refused — an archived fleet never changes again", async () => {
        const result = await runWithEnv(["fleet", "archive", fleetId, "--json"]);
        assert.notEqual(result.code, 0, `expected non-zero archiving an archived fleet; stdout=${result.stdout}`);
        assert.match(result.stderr + result.stdout, ARCHIVED_REFUSAL,
          `an archived fleet answers the archived conflict; stdout=${result.stdout} stderr=${result.stderr}`);
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
