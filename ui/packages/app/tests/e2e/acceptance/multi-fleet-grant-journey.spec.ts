/**
 * multi-fleet-grant-journey.spec.ts — two fleets, two standing grants, and the
 * credit the work spent.
 *
 * Wire: install two fleets from the gallery, each declaring a credential the
 * workspace holds as a connector handle → each install leaves its OWN approved
 * grant, one per (fleet, service), and neither raises a card → a message to the
 * first fleet leases and runs, because the permission already stands → the
 * fleet replies and its metrics strip carries tokens and a duration → the same
 * for the second fleet → the tenant's credit balance is lower than it was, and
 * the ledger says which fleets took it.
 *
 * # Why there is no card to answer
 *
 * This walk used to install, wait for an `integration_grant` card per fleet,
 * and answer each in the Approvals table. M202 retired that step:
 * `afd_approval::request::install` binds `status::APPROVED`, so an install
 * declaring a mintable credential writes the grant already granted. Installing
 * is the answer — you chose the fleet, and its bundle names the integration.
 *
 * So the journey now asserts the opposite of what it once did: no card reaches
 * the inbox, and the first delivery LEASES rather than parks. Parking still
 * exists for a fleet whose grant is missing or revoked (`Origin::Park`); it is
 * no longer where a freshly installed fleet begins.
 *
 * # Why absence is asserted after a positive signal
 *
 * "No card was raised" is unfalsifiable on its own — read early enough, every
 * fleet holds none. The walk first waits for the grant row the install-time
 * request writes, which is the same request that would once have raised the
 * card, and only then requires the inbox to be empty.
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
 * to run until an install left a grant standing.
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
import { APPROVAL_GATES_REGION_LABEL, rowForAgent } from "./fixtures/approvals-table";
import {
  chargedToFleets,
  connectorGrantFor,
  connectorTriggerMd,
  CONNECTOR_SERVICE_GITHUB,
  ensureConnectorHandle,
  GRANT_STATUS,
  type IntegrationGrant,
  KIND_INTEGRATION_GRANT,
  pendingGateFor,
  readTenantBilling,
  REASON_DECLARED_AT_INSTALL,
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
// How long a delivery may take to reach a runner before the lease is called
// missing. Comfortably past the runner's poll cadence: a shorter window would
// call a slow queue a park.
const LEASE_WINDOW_MS = 30_000;
// The install-time grant request runs after the fleet flips active, so the row
// lands a beat behind the install itself.
const GRANT_TIMEOUT_MS = 30_000;
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

// Shared by the serial chain: installed once, granted once, run once.
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

/** The standing grant a fleet's install left, or a failure naming the agent
 *  whose install wrote none. */
function standing(grant: IntegrationGrant | null, callsign: string): IntegrationGrant {
  if (grant === null) throw new Error(`agent ${callsign} holds no integration grant`);
  return grant;
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

  test("two installs each leave an approved grant for their own agent, and neither raises a card", async ({ page }) => {
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

    // One grant per (fleet, service): each install wrote its OWN fleet's row,
    // so neither can stand in for the other.
    for (const fleet of [first, second]) {
      const grant = standing(
        await pollFor(
          () =>
            observed(JOURNEY_LEG.install, () =>
              connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
            ),
          GRANT_TIMEOUT_MS,
        ),
        fleet.callsign,
      );
      expect(grant.status).toBe(GRANT_STATUS.approved);
      expect(grant.approved_at).not.toBeNull();
      expect(grant.revoked_at).toBeNull();
      // The provenance separates an install's own row from one a person
      // answered: only the install writer stamps this sentence.
      expect(grant.reason).toBe(REASON_DECLARED_AT_INSTALL);

      // Read only now the grant exists: the request that wrote it is the one
      // that would once have raised the card, so an empty inbox here is a
      // decision and not a race.
      expect(
        await observed(JOURNEY_LEG.install, () =>
          pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
        ),
        `installing ${fleet.callsign} must raise no ${KIND_INTEGRATION_GRANT} card`,
      ).toBeNull();
    }

    // The operator's own view: the inbox names neither agent, because neither
    // is waiting on anybody.
    await page.goto(workspaceHref(workspaceId, "approvals"));
    await expect(page.getByLabel(APPROVAL_GATES_REGION_LABEL)).toBeVisible({
      timeout: RENDER_TIMEOUT_MS,
    });
    for (const fleet of [first, second]) {
      await expect(rowForAgent(page, fleet.callsign)).toHaveCount(0, {
        timeout: RENDER_TIMEOUT_MS,
      });
    }
  });

  test("the first delivery leases and runs, because the grant already stands", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = installed(first);
    await signInAs(page, FIXTURE_KEY.regular);

    // A runner must be live first. This assertion inverted with M202: the walk
    // now requires a lease to APPEAR, so an offline runner would fail it for
    // the environment's reason rather than the daemon's.
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

    // No approval leg: the second fleet's install granted it too, so the only
    // thing standing between the message and the work is the work.
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
