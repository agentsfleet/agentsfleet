/**
 * grant-approval-live — the dashboard's grant walk, at the command line.
 *
 * The browser lane proves this journey against the deployed dashboard. This is
 * the same journey against the deployed API through the published CLI surface:
 *
 *   - `secret create` stores a connector handle for the credential the bundle
 *     will declare
 *   - `install --library` installs a fleet declaring it, and the install leaves
 *     an APPROVED grant behind — visible as `agentsfleet grant list --fleet <id>`
 *   - no card is raised, and none is owed: installing is the answer
 *   - `steer` then completes, and `billing show` reports a lower balance than
 *     it did before the run
 *   - `grant delete` revokes the standing permission, which is the one stop
 *     button an operator has now that no card stands in the path
 *
 * # Why no card is answered here
 *
 * This walk used to install, wait for an `integration_grant` card, and answer
 * it through `agentsfleet approvals approve`. M202 retired that step: an
 * install declaring a mintable credential writes the grant already approved
 * (`afd_approval::request::install` binds `status::APPROVED`), because you
 * chose the fleet and its bundle names the integration. So the card's absence
 * is the behaviour under test, not an omission from the walk.
 *
 * Absence is asserted after a positive signal, never after a bare sleep: the
 * walk waits until `grant list` reports the install's own row, which is the
 * same request that would have raised the card, and only then reads the inbox.
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
import { trailingJson } from "./fixtures/steer-envelope.ts";
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
  CONNECTOR_SERVICE_GITHUB,
  ensureConnectorHandle,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  KIND_INTEGRATION_GRANT,
  listGrants,
  pendingGateFor,
  REASON_DECLARED_AT_INSTALL,
  type GrantRow,
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
const GRANT_COMMAND = "grant" as const;
const DELETE_VERB = "delete" as const;
const FLEET_FLAG = "--fleet" as const;

// The install-time grant request runs after the fleet flips active, so the row
// lands a beat behind the install response.
const GRANT_TIMEOUT_MS = 30_000;
const GRANT_POLL_MS = 1_000;
// A steer's SSE round trip falls back to a ~60s poll window before rendering;
// the budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
// Install plus the wait for the install-time grant row.
const SETUP_TIMEOUT_MS = 180_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BALANCE_SETTLE_MS = 90_000;
const BALANCE_POLL_MS = 3_000;

if (!isLive) {
  describe("grant-approval-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("grant-approval-live — install, run, and pay for it, with no card in the way", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let fleetId = "";
    let ctx: AuthContext | null = null;
    let installGrant: GrantRow | null = null;
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

      // The install-time request is what would once have raised a card. Waiting
      // for the row it writes gives the later absence assertions a positive
      // ordering signal instead of a bare sleep.
      const deadline = Date.now() + GRANT_TIMEOUT_MS;
      while (installGrant === null && Date.now() < deadline) {
        const mine = (await listGrants(env, fleetId))
          .filter((row) => row.service === CONNECTOR_SERVICE_GITHUB);
        installGrant = mine[0] ?? null;
        if (installGrant === null) await Bun.sleep(GRANT_POLL_MS);
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

    it("an install declaring a connector credential leaves an approved grant the CLI can read", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_GITHUB);
      assert.equal(mine.length, 1,
        `expected one ${CONNECTOR_SERVICE_GITHUB} grant for the fleet; got ${JSON.stringify(grants)}`);
      const grant = mine[0];
      assert.equal(grant?.status, GRANT_STATUS.approved,
        `the install-time grant must land approved: ${JSON.stringify(grant)}`);
      assert.ok(grant?.approved_at,
        `an approved grant must carry approved_at: ${JSON.stringify(grant)}`);
      assert.equal(grant?.revoked_at ?? null, null,
        `a fresh install-time grant is not revoked: ${JSON.stringify(grant)}`);
      // The provenance separates this row from one a person answered: only the
      // install writer stamps this sentence.
      assert.equal(grant?.reason, REASON_DECLARED_AT_INSTALL,
        `the grant must name the install as its provenance: ${JSON.stringify(grant)}`);
    });

    it("the install raises no card, because installing is the answer", async () => {
      assert.ok(installGrant,
        `the install wrote no grant within ${GRANT_TIMEOUT_MS}ms, so the inbox read proves nothing`);
      assert.ok(ctx, "no auth context");
      const card = await pendingGateFor(ctx, fleetId);
      assert.equal(card, null,
        `an install must raise no ${KIND_INTEGRATION_GRANT} card: ${JSON.stringify(card)}`);
    });

    it("`approvals list` shows the operator an empty inbox for the fleet", async () => {
      assert.ok(installGrant, "the install wrote no grant, so an empty inbox proves nothing");
      const listed = await runFleetctl(
        ["approvals", "list", FLEET_FLAG, fleetId, JSON_FLAG],
        { env },
      );
      assert.equal(listed.code, 0, `approvals list failed: ${listed.stderr}`);
      const gates =
        (trailingJson(listed.stdout) as { items?: Array<{ gate_kind?: string }> }).items ?? [];
      const cards = gates.filter((row) => row.gate_kind === KIND_INTEGRATION_GRANT);
      assert.equal(cards.length, 0,
        `the inbox the CLI reads must carry no ${KIND_INTEGRATION_GRANT} card: ${listed.stdout}`);
    });

    it("the granted fleet answers a steer, and the tenant balance falls", async () => {
      assert.ok(fleetId, "the fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assert.equal(result.code, 0,
        `a granted fleet's steer must exit 0; stdout=${result.stdout} stderr=${result.stderr}`);
      const envelope = trailingJson(result.stdout) as Record<string, unknown>;
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

    // Last, and deliberately so: revoking the standing grant is what stops the
    // fleet reaching the provider, so every step that needs it has run by here.
    it("`grant delete` revokes the standing permission the install wrote", async () => {
      const grantId = installGrant?.id;
      assert.ok(grantId, `the install-time grant carried no id: ${JSON.stringify(installGrant)}`);
      const deleted = await runWithEnv([GRANT_COMMAND, DELETE_VERB, grantId, FLEET_FLAG, fleetId]);
      assert.equal(deleted.code, 0, `grant delete exited ${deleted.code}: ${deleted.stderr}`);

      const grants = await listGrants(env, fleetId);
      const mine = grants.filter((row) => row.service === CONNECTOR_SERVICE_GITHUB);
      assert.equal(mine.length, 1,
        `the revoke must move the row, never add one: ${JSON.stringify(grants)}`);
      assert.equal(mine[0]?.status, GRANT_STATUS.revoked,
        `the CLI still reports the grant standing: ${JSON.stringify(mine[0])}`);
      assert.ok(mine[0]?.revoked_at,
        `a revoked grant must carry revoked_at: ${JSON.stringify(mine[0])}`);
    });
  });
}
