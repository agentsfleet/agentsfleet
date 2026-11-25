// A fleet's model and network changes, lifecycle and purge through the terminal.
//
// Its fleets are morning-roosters, the prime set's plainest fleet, primed one
// after the other under that name. That a first message creates one is
// prime-morning-rooster-live.spec.ts's to prove; here the fleet is setup.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";

import { ACCEPTANCE_TARGET_ENV, AGENTSFLEET_STATUS, TERMINAL_STATUSES } from "./fixtures/constants.ts";
import {
  archiveFleet,
  expectStatus,
  getStatus,
  ARCHIVED_REFUSAL,
} from "./fixtures/lifecycle.ts";
import { primeFleet as primeFixture } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";

const fixture = primeFixture("morning-rooster");
// This file's own workspace: other files create a morning-rooster too, and a
// workspace holds one fleet of a name.
const WORKSPACE = "fixture-cli-fleet-settings";
const SETUP_TIMEOUT_MS = 120_000;

if (!isLive) {
  describe("fleet-update-delete.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("fleet update + delete + illegal transitions — seeded-credentials session", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    const env = session.env;
    const runWithEnv = session.run;

    // Scenario 1: change model and network → delete → list excludes it.
    describe("change then delete (happy path)", () => {
      let fleetId: string = "";

      beforeAll(async () => {
        fleetId = (await primeFleet(session, fixture)).fleet_id;
      }, SETUP_TIMEOUT_MS);

      it("fleet model use answers the fleet on the model it names", async () => {
        const before = await getStatus(env(), fleetId);
        const result = await runWithEnv(["fleet", "model", "use", fleetId, before.model_ref, "--json"]);
        assert.equal(result.code, 0, `model use exited ${result.code}: ${result.stderr}`);
        const fleet = JSON.parse(result.stdout) as { id?: string; model_ref?: string };
        assert.equal(fleet.id, fleetId, `model use answered another fleet: ${result.stdout}`);
        assert.equal(fleet.model_ref, before.model_ref);
      }, 30_000);

      it("fleet network use private --no-packages closes the network", async () => {
        const result = await runWithEnv(["fleet", "network", "use", fleetId, "private", "--no-packages", "--json"]);
        assert.equal(result.code, 0, `network use exited ${result.code}: ${result.stderr}`);
        const fleet = JSON.parse(result.stdout) as { http_egress?: string; package_domains?: boolean };
        assert.equal(fleet.http_egress, "restricted");
        assert.equal(fleet.package_domains, false);
      }, 30_000);

      it("a changed fleet stays active", async () => {
        await expectStatus(env(), fleetId, AGENTSFLEET_STATUS.active);
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

    // Scenario 2: archive → archive again → refused as archived (409).
    describe("archive is terminal", () => {
      let fleetId: string = "";

      beforeAll(async () => {
        fleetId = (await primeFleet(session, fixture)).fleet_id;
        await archiveFleet(env(), fleetId);
        await expectStatus(env(), fleetId, TERMINAL_STATUSES);
      }, SETUP_TIMEOUT_MS);

      it("archive <archived-id> is refused — an archived fleet never changes again", async () => {
        const result = await runWithEnv(["fleet", "archive", fleetId, "--json"]);
        assert.notEqual(result.code, 0, `expected non-zero archiving an archived fleet; stdout=${result.stdout}`);
        assert.match(result.stderr + result.stdout, ARCHIVED_REFUSAL,
          `an archived fleet answers the archived conflict; stdout=${result.stdout} stderr=${result.stderr}`);
        await expectStatus(env(), fleetId, TERMINAL_STATUSES);
      });

      it("teardown: delete the archived fixture", async () => {
        const result = await runWithEnv(["fleet", "delete", fleetId, "--json"]);
        assert.equal(result.code, 0, `delete exited ${result.code}: ${result.stderr}`);
      });
    });
  });
}
