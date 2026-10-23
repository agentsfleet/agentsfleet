import { executionInstructions } from "./fixtures/definition";
/**
 * multi-fleet-grant-journey.spec.ts — two fleets, two standing grants, and the
 * credit the work spent.
 *
 * Save two fleets with a declared connector, approve each request in the
 * inbox, and continue each saved setup. Both fleets then run real messages;
 * their own ledger rows and the tenant balance prove the work was billed.
 */
import { expect, test, type Page } from "@playwright/test";
import { deriveFleetIdentity } from "@/lib/fleets/identity";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  assertPassed,
  classifyTerminalEvent,
  failWith,
  findLeaseFor,
  JOURNEY_LEG,
  VERDICT_KIND,
  type LeaseLocation,
} from "./fixtures/execution";
import {
  awaitTerminalTurn,
  expectAnsweredOnScreen,
  messageFleet,
} from "./fixtures/chat-walk";
import { observed, pollFor, uniqueTag } from "./fixtures/observation";
import { approvalsHrefForFleet, approveRow, expectPending, rowForAgent } from "./fixtures/approvals-table";
import {
  chargedToFleets,
  connectorGrantFor,
  connectorConfig,
  ensureConnectorHandle,
  GRANT_STATUS,
  type IntegrationGrant,
  KIND_INTEGRATION_GRANT,
  pendingGateFor,
  readTenantBilling,
  REASON_DECLARED_AT_INSTALL,
} from "./fixtures/grants";
import { saveFleetViaUI } from "./fixtures/install-ui";
import { workspaceUrlPattern } from "./fixtures/nav";
import { getDefaultWorkspaceId, readFleetCounters, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

// Two stable template names — the server suffixes a repeat, and a per-run
// unique name would mint a gallery row per run that nothing deletes. One sweep
// prefix reaches both, and the suffixed fleet names still carry it.
const TEMPLATE_FIRST = "grant-walk-alpha";
const TEMPLATE_SECOND = "grant-walk-beta";
const SWEEP_PREFIX = "grant-walk";

const RENDER_TIMEOUT_MS = 15_000;
// How long a delivery may take to reach a runner before the lease is called
// missing. Comfortably past the runner's poll cadence: a shorter window would
// call a slow queue a park.
const LEASE_WINDOW_MS = 30_000;
// The install-time grant request runs after the fleet flips active, so the row
// lands a beat behind the install itself.
const GRANT_TIMEOUT_MS = 30_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BILLING_SETTLE_TIMEOUT_MS = 90_000;

// Install (60s inside saveFleetViaUI) plus the execution budgets, with headroom
// that is deliberately not another retry.
const INSTALL_TEST_TIMEOUT_MS = 240_000;
const RUN_TEST_TIMEOUT_MS = 420_000;

const MESSAGE_PREFIX = "grant-walk-probe-";

interface WalkFleet {
  readonly id: string;
  readonly callsign: string;
}

// Shared by the serial chain: installed once, granted once, run once.
let workspaceId = "";
let first: WalkFleet | null = null;
let second: WalkFleet | null = null;
let balanceBeforeNanos = 0n;

/** A fleet an earlier test in the chain installed, or that test's own failure
 * restated so the follower does not report a missing id as a product defect. */
function installed(fleet: WalkFleet | null): WalkFleet {
  if (fleet === null) throw new Error("the install step did not complete; nothing to walk");
  return fleet;
}

/** The standing grant a fleet's install left, or a failure naming the agent
 *  whose install wrote none. */
function standing(grant: IntegrationGrant | null, callsign: string): IntegrationGrant {
  if (grant === null) throw new Error(`agent ${callsign} holds no integration grant`);
  return grant;
}

/** What one reviewed save needs: a real instruction body, and runtime config
 * declaring the connector credential the workspace holds. */
function connectorInstall(template: string) {
  return {
    handle: FIXTURE_KEY.regular,
    workspaceId,
    instructions: executionInstructions(),
    config: connectorConfig(template),
  };
}

/** Wait for a freshly installed fleet to reach active, and name its agent. */
async function activated(fleetId: string): Promise<WalkFleet> {
  await observed(JOURNEY_LEG.install, () =>
    waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
  );
  return { id: fleetId, callsign: deriveFleetIdentity(fleetId).callsign };
}

/** Approval is separate from Save; continuing must keep the original fleet. */
async function approveAndContinue(page: Page, fleetId: string, setupUrl: string): Promise<void> {
  const callsign = deriveFleetIdentity(fleetId).callsign;
  const grant = standing(await connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleetId), callsign);
  expect(grant.status).toBe(GRANT_STATUS.pending);
  expect(grant.approved_at).toBeNull();
  expect(grant.revoked_at).toBeNull();
  expect(grant.reason).toBe(REASON_DECLARED_AT_INSTALL);
  const gate = await pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleetId);
  if (!gate) throw new Error(`saved fleet ${callsign} has no approval request`);
  await page.goto(approvalsHrefForFleet(workspaceId, fleetId));
  const row = rowForAgent(page, callsign);
  await expectPending(row, RENDER_TIMEOUT_MS);
  await approveRow(row, gate.proposed_action, RENDER_TIMEOUT_MS);
  const approved = standing(await connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleetId), callsign);
  expect(approved.status).toBe(GRANT_STATUS.approved);
  expect(approved.approved_at).not.toBeNull();
  expect(await pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleetId)).toBeNull();
  await page.goto(setupUrl);
  await page.getByRole("button", {name: "Continue setup", exact: true}).click();
  await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`), {timeout: GRANT_TIMEOUT_MS});
}

test.describe.serial("multi-fleet grant journey", () => {
  test.afterAll(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, SWEEP_PREFIX);
  });

  test("two saved fleets each require their own approval and continuation", async ({ page }) => {
    test.setTimeout(INSTALL_TEST_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await observed(JOURNEY_LEG.install, () =>
      ensureConnectorHandle(FIXTURE_KEY.regular, workspaceId),
    );
    // Read before anything runs: the last test spends from this figure.
    const billing = await observed(JOURNEY_LEG.install, () =>
      readTenantBilling(FIXTURE_KEY.regular),
    );
    if (billing.is_exhausted) {
      failWith({
        kind: VERDICT_KIND.environment,
        leg: JOURNEY_LEG.install,
        detail: "the fixture tenant's credit is exhausted; no run can be billed",
      });
    }
    balanceBeforeNanos = BigInt(billing.balance_nanos);

    await signInAs(page, FIXTURE_KEY.regular);
    // The template argument is spelled as a constant at the call site: a
    // per-run unique name mints a gallery row per run that nothing deletes.
    const firstId = await saveFleetViaUI(page, TEMPLATE_FIRST, connectorInstall(TEMPLATE_FIRST));
    const firstSetupUrl = page.url();
    const secondId = await saveFleetViaUI(page, TEMPLATE_SECOND, connectorInstall(TEMPLATE_SECOND));
    const secondSetupUrl = page.url();
    await approveAndContinue(page, firstId, firstSetupUrl);
    await approveAndContinue(page, secondId, secondSetupUrl);
    first = await activated(firstId);
    second = await activated(secondId);
    expect(first.callsign, "two fleets must not share one callsign").not.toBe(second.callsign);
  });

  test("the first delivery leases and runs, because the grant already stands", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = installed(first);
    await signInAs(page, FIXTURE_KEY.regular);

    // An online runner distinguishes a blocked grant from missing capacity.
    expect(
      await observed(JOURNEY_LEG.lease, () => anyRunnerLive()),
      "no runner is online, so an unleased delivery says nothing about the grant",
    ).toBe(true);

    await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    const leased = await pollFor<LeaseLocation>(
      () => observed(JOURNEY_LEG.lease, () => findLeaseFor(fleet.id)),
      LEASE_WINDOW_MS,
    );
    expect(
      leased,
      "a delivery whose grant already stands must lease, not park",
    ).not.toBeNull();

    // Every model turn is its own event. A continuation that asked again would
    // raise a card mid-run, which is the defect M202 closed — so the inbox is
    // read after the run, not only after the install.
    assertPassed(classifyTerminalEvent(await awaitTerminalTurn(workspaceId, fleet.id)));
    expect(
      await observed(JOURNEY_LEG.observe, () =>
        pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      `no turn of ${fleet.callsign}'s run may raise a ${KIND_INTEGRATION_GRANT} card`,
    ).toBeNull();

    // The grant the run minted against is the install's own, still standing and
    // still one row: a run must not write a second.
    const after = standing(
      await observed(JOURNEY_LEG.observe, () =>
        connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      fleet.callsign,
    );
    expect(after.status).toBe(GRANT_STATUS.approved);

    await expectAnsweredOnScreen(page, workspaceId, fleet.id);
  });

  test("the second agent answers its own chat, and the tenant credit balance falls", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = installed(second);
    const both = [installed(first).id, fleet.id];
    await signInAs(page, FIXTURE_KEY.regular);

    // The second fleet has its own explicit approval from the first test.
    standing(
      await observed(JOURNEY_LEG.observe, () =>
        connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      fleet.callsign,
    );

    await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    assertPassed(classifyTerminalEvent(await awaitTerminalTurn(workspaceId, fleet.id)));
    await expectAnsweredOnScreen(page, workspaceId, fleet.id);

    // Both fleets spent something of their own, so the fall below is theirs and
    // not a parallel worker's.
    for (const fleetId of both) {
      const counters = await observed(JOURNEY_LEG.observe, () =>
        readFleetCounters(FIXTURE_KEY.regular, workspaceId, fleetId),
      );
      expect(BigInt(counters.events_processed), `fleet ${fleetId} processed no event`).toBeGreaterThan(0n);
    }
    expect(
      await observed(JOURNEY_LEG.observe, () => chargedToFleets(FIXTURE_KEY.regular, both)),
      "the usage ledger carries no charge for either fleet",
    ).toBeGreaterThan(0n);

    // Nanos, not the rendered card: the balance card rounds to cents, and a run
    // costing a tenth of one would leave the figure unchanged.
    await expect
      .poll(
        async () =>
          BigInt((await observed(JOURNEY_LEG.observe, () => readTenantBilling(FIXTURE_KEY.regular))).balance_nanos),
        { timeout: BILLING_SETTLE_TIMEOUT_MS },
      )
      .toBeLessThan(balanceBeforeNanos);
  });
});
