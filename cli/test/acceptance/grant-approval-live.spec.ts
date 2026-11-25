/**
 * A fleet asks for a connector with `request_access`, and a person approves
 * it once from the terminal. The fleet starts with `fleet create`, its first
 * message telling it to ask; the approval resumes that turn holding the
 * access. Then it runs a real message, the spending shows up, and the access
 * is revoked. The spending is read the way a terminal user reads it: `usage
 * show` prints the run's charge as a row, and its cursor pages back without
 * repeating one.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_TARGET_ENV, AGENTSFLEET_STATUS } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, refusalCode } from "./fixtures/negatives.ts";
import { trailingJson } from "./fixtures/steer-envelope.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { addWorkspace, listWorkspaces, useWorkspace } from "./fixtures/workspace-ops.ts";
import { createFleetFromCli } from "./fixtures/fleet-create.ts";
import { getStatus } from "./fixtures/lifecycle.ts";
import { deleteFixtureFleet } from "./fixtures/teardown.ts";
import { readAuthContext, type AuthContext } from "./fixtures/native-api.ts";
import { chargedNanos, readUsagePage, readUsageTextTotal } from "./fixtures/usage-ops.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";
import { instantMs } from "../../src/lib/instant.ts";
import {
  ACCESS_REQUEST_MESSAGE,
  CONNECTOR_SERVICE_LINEAR,
  ensureConnectorHandle,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  KIND_INTEGRATION_GRANT,
  listGrants,
  pendingGateFor,
  waitForPendingGate,
  type GrantRow,
} from "./fixtures/grant-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-grant-" as const;
const GRANT_WORKSPACE = "fixture-cli-connector-grants";
const FLEET_NAME = "linear-issue-triage";
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const STEER_COMMAND = ["fleet", "steer"] as const;
const BILLING_ARGS = ["usage", "show", JSON_FLAG] as const;
const BALANCE_FIELD = "balance_nanos" as const;
const ONE_SHOT_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const ENVELOPE_STATUS_KEY = "status" as const;
const ENVELOPE_TURN_ID_KEY = "turn_id" as const;
// A steer that lands while the approval's resumed run still goes joins that run
// (`folded_into_turn_id`, afd_turns/src/view/steer/disposition.rs), and a run
// bills its lines under its own head turn (afd_billing/src/ledger/lines.rs).
const ENVELOPE_FOLDED_INTO_KEY = "folded_into_turn_id" as const;
const STATUS_PROCESSED = "processed" as const;
const GRANT_COMMAND = ["fleet", "grants"] as const;
const REVOKE_VERB = "revoke" as const;
const FLEET_FLAG = "--fleet" as const;

// A steer's SSE round trip falls back to a ~60s poll window before rendering;
// the budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
// The create plus the first run reaching its `request_access` call.
const SETUP_TIMEOUT_MS = 180_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BALANCE_SETTLE_MS = 90_000;
const BALANCE_POLL_MS = 3_000;
// A dollar figure as `formatDollars` (src/constants/billing.ts) renders one.
const DOLLARS = /^-?\$[\d,]+\.\d{2,}$/u;
// Decodes to no boundary the charges route can read, so no page issued it
// (`afd_billing::tenant::cursor::parse`).
const FOREIGN_CURSOR = "not-a-cursor" as const;
// The charges route refuses a cursor it cannot parse as INVALID_REQUEST
// (afd_billing/src/error/mod.rs).
const CODE_INVALID_REQUEST = "UZ-REQ-001" as const;

const linearOf = (grants: ReadonlyArray<GrantRow>) => grants.filter((row) => row.name === CONNECTOR_SERVICE_LINEAR);

if (!isLive) {
  describe("grant-approval-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("grant-approval-live — the fleet asks, a person approves once, it runs, access is revoked", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let fleetId = "";
    let ctx: AuthContext | null = null;
    let requestedGrant: GrantRow | null = null;
    let gateId = "";
    let balanceBeforeNanos = 0n;
    let steeredTurnId = "";
    // The turn the steer's work is billed under: its own, or the run it joined.
    let billedTurnId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: STEER_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function readBalanceNanos(): Promise<bigint> {
      const result = await runWithEnv([...BILLING_ARGS]);
      assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
      const balance = parsed[BALANCE_FIELD];
      assert.equal(typeof balance, "string", `billing show carried no ${BALANCE_FIELD}`);
      assert.match(String(balance), /^(0|[1-9][0-9]*)$/);
      return BigInt(String(balance));
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), {
        email: resolveFixtureEmail("regular"),
      });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR,
      });
      workspaceId = (await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir }))
        .currentWorkspaceId;
      const existing = (await listWorkspaces(env)).find(workspace => workspace.name === GRANT_WORKSPACE);
      workspaceId = existing?.workspace_id ?? (await addWorkspace(env, GRANT_WORKSPACE)).workspaceId;
      await useWorkspace(env, workspaceId);
      ctx = await readAuthContext(env);

      balanceBeforeNanos = await readBalanceNanos();
      await ensureConnectorHandle(env);
      const deadline = Date.now() + SETUP_TIMEOUT_MS;
      fleetId = (await createFleetFromCli(env, { name: FLEET_NAME, message: ACCESS_REQUEST_MESSAGE })).fleet_id;
      gateId = (await waitForPendingGate(ctx, fleetId, deadline)).gate_id;
      requestedGrant = linearOf(await listGrants(env, fleetId))[0] ?? null;
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      try {
        if (workspaceId) {
          if (fleetId) await deleteFixtureFleet(env, fleetId);
          const removed = await runFleetctl(["secrets", "delete", GRANT_CREDENTIAL_NAME, JSON_FLAG], {env});
          assert.equal(removed.code, 0, `synthetic connector cleanup failed: ${removed.stderr}`);
        }
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
      }
    });

    it("the fleet's request leaves one pending grant, named for what it asked", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      const grants = await listGrants(env, fleetId);
      const mine = linearOf(grants);
      assert.equal(mine.length, 1,
        `expected one ${CONNECTOR_SERVICE_LINEAR} grant for the fleet; got ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.pending, `asking is not consent: ${JSON.stringify(mine[0])}`);
      assert.equal(mine[0]?.approved_at ?? null, null,
        `a pending grant cannot carry approved_at: ${JSON.stringify(mine[0])}`);
    });

    it("the CLI inbox shows the pending access request for this fleet", async () => {
      assert.ok(requestedGrant, "the request wrote no grant");
      const listed = await runFleetctl(
        ["fleet", "approvals", "list", FLEET_FLAG, fleetId, JSON_FLAG],
        { env },
      );
      assert.equal(listed.code, 0, `approvals list failed: ${listed.stderr}`);
      const gates =
        (trailingJson(listed.stdout) as { items?: Array<{ gate_type?: string }> }).items ?? [];
      const cards = gates.filter((row) => row.gate_type === KIND_INTEGRATION_GRANT);
      assert.equal(cards.length, 1,
        `the inbox must carry this fleet's ${KIND_INTEGRATION_GRANT} request: ${listed.stdout}`);
    });

    it("one approval grants the access and answers the asking turn", async () => {
      assert.ok(ctx && gateId, "no access request was raised");
      const approved = await runWithEnv(["fleet", "approvals", "approve", gateId, JSON_FLAG]);
      assert.equal(approved.code, 0, `approval failed: ${approved.stderr}`);
      const grant = linearOf(await listGrants(env, fleetId))[0];
      assert.equal(grant?.status, GRANT_STATUS.approved);
      assert.ok(grant?.approved_at, `an approved grant must carry approved_at: ${JSON.stringify(grant)}`);
      assert.equal(await pendingGateFor(ctx, fleetId), null);
      assert.equal((await getStatus(env, fleetId)).status, AGENTSFLEET_STATUS.active);
    });

    it("the granted fleet answers a steer, and the tenant balance falls", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      const result = await runWithEnv([...STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assert.equal(result.code, 0,
        `a granted fleet's steer must exit 0; stdout=${result.stdout} stderr=${result.stderr}`);
      const envelope = trailingJson(result.stdout) as Record<string, unknown>;
      assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
        `expected ${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);
      const turnId = envelope[ENVELOPE_TURN_ID_KEY];
      assert.equal(typeof turnId, "string", `the steer named no work: ${JSON.stringify(envelope)}`);
      steeredTurnId = String(turnId);
      const joined = envelope[ENVELOPE_FOLDED_INTO_KEY];
      billedTurnId = typeof joined === "string" ? joined : steeredTurnId;

      // Nanos, never a rendered figure: a run costing a fraction of a cent
      // moves the balance and no two-decimal display.
      const deadline = Date.now() + BALANCE_SETTLE_MS;
      let balanceAfterNanos = await readBalanceNanos();
      while (balanceAfterNanos >= balanceBeforeNanos && Date.now() < deadline) {
        await Bun.sleep(BALANCE_POLL_MS);
        balanceAfterNanos = await readBalanceNanos();
      }
      assert.ok(balanceAfterNanos < balanceBeforeNanos,
        `the run did not deplete credit: ${balanceBeforeNanos} → ${balanceAfterNanos} nanos`);
    }, STEER_TIMEOUT_MS);

    it("`usage show` prints a dollar total for the run and JSON shows a positive charge for this fleet", async () => {
      assert.ok(billedTurnId, "the steer did not run");
      const printedTotal = await readUsageTextTotal(runWithEnv, billedTurnId);
      assert.match(printedTotal, DOLLARS, `the run's total is not a dollar figure: ${printedTotal}`);

      const charged = await chargedNanos(runWithEnv, fleetId, billedTurnId);
      assert.ok(charged > 0n, `usage show charged the run nothing: ${charged} nanos`);
    }, STEER_TIMEOUT_MS);

    it("`usage show --cursor` continues to an older charge without repeating the first page's", async () => {
      assert.ok(steeredTurnId, "the steer did not run");
      const first = await readUsagePage(runWithEnv, 1);
      assert.equal(first.events.length, 1, `a one-row page grouped into ${first.events.length} entries`);
      assert.ok(first.next_cursor, "a full page must offer the cursor that continues it");
      // A run writes a model row per request and the daemon adds a runtime
      // row (af_wire/src/usage/mod.rs), so the run alone fills a second page.
      const second = await readUsagePage(runWithEnv, 1, first.next_cursor);
      assert.equal(second.events.length, 1, "the page after a full one came back empty");
      // The daemon renders a cursor from its page's last row, so an equal
      // cursor means page two handed back page one's row.
      assert.notEqual(second.next_cursor, first.next_cursor, "page two repeated page one's row");
      const newer = instantMs(first.events[0]?.recorded_at);
      const older = instantMs(second.events[0]?.recorded_at);
      assert.ok(newer !== null && older !== null && older <= newer,
        `the walk went forward in time: ${JSON.stringify([first, second])}`);
    });

    it("`usage show --cursor` with a token no page issued is refused by the daemon", async () => {
      const refused = await runWithEnv(["usage", "show", "--cursor", FOREIGN_CURSOR, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a foreign cursor was accepted: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_INVALID_REQUEST);
    });

    // Last, and deliberately so: revoking the standing grant is what stops the
    // fleet reaching the provider, so every step that needs it has run by here.
    it("`fleet grants revoke` revokes the standing permission the approval gave", async () => {
      const grantId = requestedGrant?.id;
      assert.ok(grantId, `the requested grant carried no id: ${JSON.stringify(requestedGrant)}`);
      const revoked = await runWithEnv([...GRANT_COMMAND, REVOKE_VERB, grantId, FLEET_FLAG, fleetId]);
      assert.equal(revoked.code, 0, `fleet grants revoke exited ${revoked.code}: ${revoked.stderr}`);

      const grants = await listGrants(env, fleetId);
      const mine = linearOf(grants);
      assert.equal(mine.length, 1,
        `the revoke must move the row, never add one: ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.revoked,
        `the CLI still reports the grant standing: ${JSON.stringify(mine[0])}`);
    });
  });
}
