/**
 * grant-approval-live — the dashboard's grant walk, at the command line.
 *
 * The browser lane proves this journey against the deployed dashboard. This is
 * the same journey against the deployed API through the published CLI surface:
 *
 *   - `secret create` stores a connector handle for the credential the bundle
 *     will declare
 *   - `install --library` installs a fleet declaring it, and the install leaves
 *     a PENDING grant behind — visible as `agentsfleet grant list --fleet <id>`
 *   - the card is answered, and `grant list` reports the SAME grant approved,
 *     with `approved_at` set: the stage change an operator watches for
 *   - `steer` then completes, and `billing show` reports a lower balance than
 *     it did before the run
 *
 * # The one step the CLI cannot take
 *
 * `agentsfleet grant` ships `list` and `delete`. There is no verb that answers
 * a pending card, so the approval below goes over HTTP (see `grant-ops.ts`).
 * The walk still grades the CLI on both sides of that decision, which is what
 * makes the gap legible rather than papered over.
 *
 * # Why the balance is compared as an integer
 *
 * `billing show --json` answers `balance_nanos`. A run charged a fraction of a
 * cent moves it and moves no rendered currency figure, so the comparison is on
 * the integer the API returns and never on formatted output.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { trailingJsonObject } from "./fixtures/steer-envelope.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installConnectorProbeFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import { readAuthContext, type AuthContext } from "./fixtures/template-ops.ts";
import {
  approveGate,
  CONNECTOR_SERVICE_GITHUB,
  ensureConnectorHandle,
  GATE_STATUS,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  KIND_INTEGRATION_GRANT,
  listGrants,
  pendingGateFor,
  type GateRow,
} from "./fixtures/grant-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-grant-" as const;
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const STEER_COMMAND = "steer" as const;
const BILLING_ARGS = ["billing", "show", JSON_FLAG] as const;
const BALANCE_FIELD = "balance_nanos" as const;
const ONE_SHOT_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const ENVELOPE_STATUS_KEY = "status" as const;
const STATUS_PROCESSED = "processed" as const;
// The evidence key the approve statement joins the grant row on. Mirrors
// `afd_approval::request::EVIDENCE_SERVICE`.
const EVIDENCE_SERVICE = "service" as const;

// The install-time grant request runs after the fleet flips active, so the card
// lands a beat behind the install response.
const CARD_TIMEOUT_MS = 30_000;
const CARD_POLL_MS = 1_000;
// A steer's SSE round trip falls back to a ~60s poll window before rendering;
// the budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
// Install plus the card wait.
const SETUP_TIMEOUT_MS = 180_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BALANCE_SETTLE_MS = 90_000;
const BALANCE_POLL_MS = 3_000;

if (!isLive) {
  describe("grant-approval-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("grant-approval-live — install, answer the card, run, and pay for it", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let fleetId = "";
    let ctx: AuthContext | null = null;
    let card: GateRow | null = null;
    let balanceBeforeNanos = 0;

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: STEER_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function readBalanceNanos(): Promise<number> {
      const result = await runWithEnv([...BILLING_ARGS]);
      assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
      const balance = parsed[BALANCE_FIELD];
      assert.equal(typeof balance, "number", `billing show carried no ${BALANCE_FIELD}`);
      return balance as number;
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
      ctx = await readAuthContext(env);

      balanceBeforeNanos = await readBalanceNanos();
      await ensureConnectorHandle(env);
      const installed = await installConnectorProbeFleet({ env }, GRANT_CREDENTIAL_NAME);
      const id = installed.id ?? installed.fleet_id;
      if (!id) throw new Error(`install missing id: ${JSON.stringify(installed)}`);
      fleetId = id;

      const deadline = Date.now() + CARD_TIMEOUT_MS;
      while (card === null && Date.now() < deadline) {
        card = await pendingGateFor(ctx, fleetId);
        if (card === null) await Bun.sleep(CARD_POLL_MS);
      }
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("an install declaring a connector credential leaves a pending grant the CLI can read", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_GITHUB);
      assert.equal(mine.length, 1,
        `expected one ${CONNECTOR_SERVICE_GITHUB} grant for the fleet; got ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.pending,
        `the install-time grant must start pending: ${JSON.stringify(mine[0])}`);
    });

    it("the card raised alongside it names the service the approve statement reads", () => {
      assert.ok(card, `no ${KIND_INTEGRATION_GRANT} card was raised for fleet ${fleetId}`);
      assert.equal(card.gate_kind, KIND_INTEGRATION_GRANT);
      assert.equal(card.fleet_id, fleetId);
      assert.equal(card.status, GATE_STATUS.pending);
      assert.equal(card.evidence[EVIDENCE_SERVICE], CONNECTOR_SERVICE_GITHUB,
        `a card without the service key resolves cleanly and moves no grant: ${JSON.stringify(card.evidence)}`);
      assert.ok(card.proposed_action.includes(CONNECTOR_SERVICE_GITHUB),
        `the headline must name the service: ${card.proposed_action}`);
    });

    it("answering the card moves the same grant to approved", async () => {
      assert.ok(card && ctx, "no card to answer");
      const decided = await approveGate(ctx, card.gate_id);
      assert.equal(decided.status ?? GATE_STATUS.approved, GATE_STATUS.approved,
        `the resolve did not answer approved: ${JSON.stringify(decided)}`);

      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_GITHUB);
      assert.equal(mine.length, 1, `the grant was duplicated: ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.approved,
        `the CLI still reports the grant unapproved: ${JSON.stringify(mine[0])}`);
      assert.ok(mine[0]?.approved_at, `an approved grant must carry approved_at: ${JSON.stringify(mine[0])}`);
    });

    it("the granted fleet answers a steer, and the tenant balance falls", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assert.equal(result.code, 0,
        `a granted fleet's steer must exit 0; stdout=${result.stdout} stderr=${result.stderr}`);
      const envelope = JSON.parse(trailingJsonObject(result.stdout)) as Record<string, unknown>;
      assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
        `expected ${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);

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
  });
}
