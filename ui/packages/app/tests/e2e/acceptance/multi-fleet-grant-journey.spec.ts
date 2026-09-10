/**
 * multi-fleet-grant-journey.spec.ts — two fleets, two cards, two answers, and
 * the credit the work spent.
 *
 * Wire: install two fleets from the gallery, each declaring a credential the
 * workspace holds as a connector handle → each install raises its OWN pending
 * card, one per (fleet, service) → a message to the first fleet parks, because
 * a pending grant is a question nobody has answered → answering the card in the
 * Approvals table moves the row and unparks the delivery → the fleet replies
 * and its metrics strip carries tokens and a duration → the same for the second
 * fleet → the tenant's credit balance is lower than it was, and the ledger says
 * which fleets took it.
 *
 * # Why the park is asserted as an ABSENCE
 *
 * "The delivery parks" is not a visible state; what is visible is that no
 * runner ever leases it. So the walk waits a named window for a lease and
 * requires none to appear, having first established that a runner is live —
 * without that check an offline runner would prove the park for free.
 *
 * # Why the balance is read in nanos
 *
 * The billing card rounds to cents. A run costing a tenth of a cent moves the
 * balance and not the rendered figure, so a walk scraping the card would report
 * a regression on every cheap run. The balance is compared in nanos, and the
 * fall is attributed through the ledger's own `fleet_id` — the fixture tenant
 * is shared across parallel workers, so a falling balance alone would say only
 * that SOMETHING ran.
 *
 * The tests are serial and share the two installed fleets: each provider round
 * trip is real money and a real minute, and the claims stack — there is nothing
 * to approve until an install raised a card.
 */
import { expect, test, type Page } from "@playwright/test";
import { APPROVAL_STATUS } from "@/lib/api/approvals-types";
import type { ApprovalGate } from "@/lib/api/approvals";
import { deriveFleetIdentity } from "@/app/(dashboard)/w/[workspaceId]/fleets/components/fleetIdentity";
import { fixtureSubject, signInAs } from "./fixtures/auth";
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
import {
  approvalsHrefForFleet,
  approveRow,
  APPROVAL_GATES_REGION_LABEL,
  expectDecidedBy,
  expectPending,
  rowForAgent,
} from "./fixtures/approvals-table";
import {
  chargedToFleets,
  connectorTriggerMd,
  CONNECTOR_SERVICE_GITHUB,
  ensureConnectorHandle,
  KIND_INTEGRATION_GRANT,
  pendingGateFor,
  readGate,
  readTenantBilling,
  serviceOn,
} from "./fixtures/grants";
import { installViaUI } from "./fixtures/install-ui";
import { workspaceHref } from "./fixtures/nav";
import {
  executionSkillMd,
  getDefaultWorkspaceId,
  readFleetCounters,
  waitForFleetActive,
} from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

// Two stable template names — the server suffixes a repeat, and a per-run
// unique name would mint a gallery row per run that nothing deletes. One sweep
// prefix reaches both, and the suffixed fleet names still carry it.
const TEMPLATE_FIRST = "grant-walk-alpha";
const TEMPLATE_SECOND = "grant-walk-beta";
const SWEEP_PREFIX = "grant-walk";

const RENDER_TIMEOUT_MS = 15_000;
// How long a parked delivery must go unleased before the park is believed.
// Comfortably past the runner's poll cadence: a shorter window would call a
// slow queue a park.
const PARK_WINDOW_MS = 30_000;
// The install-time grant request runs after the fleet flips active, so the card
// lands a beat behind the install itself.
const CARD_TIMEOUT_MS = 30_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BILLING_SETTLE_TIMEOUT_MS = 90_000;

// Install (60s inside installViaUI) plus the execution budgets, with headroom
// that is deliberately not another retry.
const INSTALL_TEST_TIMEOUT_MS = 240_000;
const RUN_TEST_TIMEOUT_MS = 420_000;

const MESSAGE_PREFIX = "grant-walk-probe-";

interface WalkFleet {
  readonly id: string;
  readonly callsign: string;
}

// Shared by the serial chain: installed once, answered once, run once.
let workspaceId = "";
let first: WalkFleet | null = null;
let second: WalkFleet | null = null;
let balanceBeforeNanos = 0;

/** A fleet an earlier test in the chain installed, or that test's own failure
 * restated so the follower does not report a missing id as a product defect. */
function installed(fleet: WalkFleet | null): WalkFleet {
  if (fleet === null) throw new Error("the install step did not complete; nothing to walk");
  return fleet;
}

/** The card a fleet is waiting on, or a failure naming the agent that has none. */
function raised(gate: ApprovalGate | null, callsign: string): ApprovalGate {
  if (gate === null) throw new Error(`agent ${callsign} holds no pending card`);
  return gate;
}

/** What one gallery install needs: a real instruction body, and frontmatter
 * declaring the connector credential the workspace holds. */
function connectorInstall(template: string) {
  return {
    handle: FIXTURE_KEY.regular,
    workspaceId,
    skillMarkdown: executionSkillMd(template),
    triggerMarkdown: connectorTriggerMd(template),
  };
}

/** Wait for a freshly installed fleet to reach active, and name its agent. */
async function activated(fleetId: string): Promise<WalkFleet> {
  await observed(JOURNEY_LEG.install, () =>
    waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
  );
  return { id: fleetId, callsign: deriveFleetIdentity(fleetId).callsign };
}

test.describe.serial("multi-fleet grant journey", () => {
  test.afterAll(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, SWEEP_PREFIX);
  });

  test("two installs each raise a pending grant card for their own agent", async ({ page }) => {
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
    balanceBeforeNanos = billing.balance_nanos;

    await signInAs(page, FIXTURE_KEY.regular);
    // The template argument is spelled as a constant at the call site: a
    // per-run unique name mints a gallery row per run that nothing deletes.
    const firstId = await installViaUI(page, TEMPLATE_FIRST, connectorInstall(TEMPLATE_FIRST));
    const secondId = await installViaUI(page, TEMPLATE_SECOND, connectorInstall(TEMPLATE_SECOND));
    first = await activated(firstId);
    second = await activated(secondId);
    expect(first.callsign, "two fleets must not share one callsign").not.toBe(second.callsign);

    // One card per (fleet, service): each install asked for its OWN fleet's
    // grant, so neither answer can stand in for the other.
    for (const fleet of [first, second]) {
      const gate = raised(
        await pollFor(
          () =>
            observed(JOURNEY_LEG.install, () =>
              pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
            ),
          CARD_TIMEOUT_MS,
        ),
        fleet.callsign,
      );
      expect(gate.gate_kind).toBe(KIND_INTEGRATION_GRANT);
      expect(serviceOn(gate)).toBe(CONNECTOR_SERVICE_GITHUB);
      expect(gate.fleet_id).toBe(fleet.id);
    }

    // The operator's own view: both cards, in one workspace inbox, each naming
    // a different agent.
    await page.goto(workspaceHref(workspaceId, "approvals"));
    await expect(page.getByLabel(APPROVAL_GATES_REGION_LABEL)).toBeVisible({
      timeout: RENDER_TIMEOUT_MS,
    });
    for (const fleet of [first, second]) {
      await expectPending(rowForAgent(page, fleet.callsign), RENDER_TIMEOUT_MS);
    }
  });

  test("a pending card parks the delivery, and answering it moves the row and runs the work", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = installed(first);
    await signInAs(page, FIXTURE_KEY.regular);

    // A runner must be live first: with none, "nothing leased it" would be the
    // environment agreeing with the assertion for the wrong reason.
    expect(
      await observed(JOURNEY_LEG.lease, () => anyRunnerLive()),
      "no runner is online, so an unleased delivery proves nothing about the park",
    ).toBe(true);

    await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    const leasedWhilePending = await pollFor<LeaseLocation>(
      () => observed(JOURNEY_LEG.lease, () => findLeaseFor(fleet.id)),
      PARK_WINDOW_MS,
    );
    expect(
      leasedWhilePending,
      "a delivery whose grant is still pending must park, not lease",
    ).toBeNull();

    const gate = raised(
      await observed(JOURNEY_LEG.observe, () =>
        pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      fleet.callsign,
    );
    const decider = fixtureSubject(FIXTURE_KEY.regular);

    await page.goto(approvalsHrefForFleet(workspaceId, fleet.id));
    const row = rowForAgent(page, fleet.callsign);
    await expectPending(row, RENDER_TIMEOUT_MS);
    await approveRow(row, gate.proposed_action, RENDER_TIMEOUT_MS);
    await expectDecidedBy(row, decider, RENDER_TIMEOUT_MS);

    // The same decision as the daemon recorded it, read back off the card.
    const answered = await observed(JOURNEY_LEG.observe, () =>
      readGate(FIXTURE_KEY.regular, workspaceId, gate.gate_id),
    );
    expect(answered.status).toBe(APPROVAL_STATUS.APPROVED);
    expect(answered.resolved_by).toBe(decider);

    // The parked delivery is still the one that runs: the card carries no event
    // id, so answering it lands no second copy of the work.
    assertPassed(classifyTerminalEvent(await awaitTerminalTurn(workspaceId, fleet.id)));
    await expectAnsweredOnScreen(page, workspaceId, fleet.id);
  });

  test("the second agent answers its own chat, and the tenant credit balance falls", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = installed(second);
    const both = [installed(first).id, fleet.id];
    await signInAs(page, FIXTURE_KEY.regular);

    const gate = raised(
      await observed(JOURNEY_LEG.observe, () =>
        pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      fleet.callsign,
    );
    await page.goto(approvalsHrefForFleet(workspaceId, fleet.id));
    await approveRow(rowForAgent(page, fleet.callsign), gate.proposed_action, RENDER_TIMEOUT_MS);

    await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    assertPassed(classifyTerminalEvent(await awaitTerminalTurn(workspaceId, fleet.id)));
    await expectAnsweredOnScreen(page, workspaceId, fleet.id);

    // Both fleets spent something of their own, so the fall below is theirs and
    // not a parallel worker's.
    for (const fleetId of both) {
      const counters = await observed(JOURNEY_LEG.observe, () =>
        readFleetCounters(FIXTURE_KEY.regular, workspaceId, fleetId),
      );
      expect(counters.events_processed, `fleet ${fleetId} processed no event`).toBeGreaterThan(0);
    }
    expect(
      await observed(JOURNEY_LEG.observe, () => chargedToFleets(FIXTURE_KEY.regular, both)),
      "the usage ledger carries no charge for either fleet",
    ).toBeGreaterThan(0);

    // Nanos, not the rendered card: the balance card rounds to cents, and a run
    // costing a tenth of one would leave the figure unchanged.
    await expect
      .poll(
        async () =>
          (await observed(JOURNEY_LEG.observe, () => readTenantBilling(FIXTURE_KEY.regular)))
            .balance_nanos,
        { timeout: BILLING_SETTLE_TIMEOUT_MS },
      )
      .toBeLessThan(balanceBeforeNanos);
  });
});
