/**
 * multi-fleet-grant-journey.spec.ts — two fleets, two standing grants, and the
 * credit the work spent.
 *
 * Two fleets each ask for the workspace's Linear handle with `request_access`;
 * each request raises its own card, a person approves each once in that
 * fleet's chat, and the waiting run resumes. Both fleets then run real
 * messages; their own ledger rows and the tenant balance prove the work was
 * billed.
 */
import { expect, test, type Page } from "@playwright/test";
import { fleetCallsign } from "@/lib/fleets/identity";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  assertPassed,
  classifyTerminalTurn,
  failWith,
  findRunFor,
  JOURNEY_LEG,
  VERDICT_KIND,
  type FailedVerdict,
  type RunLocation,
} from "./fixtures/execution";
import {
  awaitTerminalTurn,
  expectAnsweredOnScreen,
  messageFleet,
} from "./fixtures/chat-walk";
import { observed, pollFor, uniqueTag } from "./fixtures/observation";
import { approveFleetRequest } from "./fixtures/fleet-approval";
import {
  chargedToFleets,
  connectorGrantFor,
  ensureConnectorHandle,
  GRANT_CREDENTIAL_NAME,
  GRANT_STATUS,
  type IntegrationGrant,
  KIND_INTEGRATION_GRANT,
  pendingGateFor,
  readTenantBilling,
} from "./fixtures/grants";
import { createFixtureFleet } from "./fixtures/fleet-create";
import { accessMessage } from "./fixtures/probe";
import { ensureSecondWorkspace, readFleetCounters, expectFleetActive } from "./fixtures/seed";
import { clientFor } from "./fixtures/api-client";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { catalogueLabels, fleetUsage, type FleetUsage } from "./fixtures/usage";
import { ALL_FLEETS_LABEL } from "@/app/(dashboard)/settings/billing/lib/report-format";
import { modelLabel } from "@/lib/model-labels";

// Two purpose names, one per fleet; one sweep prefix reaches both, and a name
// a leaked sibling still holds is moved to `-2` by the create fixture.
// A settled charge can be a fraction of a cent ("$0.0001"), so only an exact zero counts as none.
const ZERO_CHARGE = /\$0\.00(?!\d)/;
const TEMPLATE_FIRST = "grant-walk-alpha";
const TEMPLATE_SECOND = "grant-walk-beta";
const SWEEP_PREFIX = "grant-walk";
const GRANT_WORKSPACE = "fixture-browser-connector-grants";

const RENDER_TIMEOUT_MS = 15_000;
// How long a delivery may take to reach a runner before the run is called
// missing. Comfortably past the runner's poll cadence: a shorter window would
// call a slow queue a park.
const RUN_WINDOW_MS = 30_000;
// The first message's run has to reach a runner and call request_access
// before its card exists: the queue plus one provider round trip.
const REQUEST_TIMEOUT_MS = 180_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BILLING_SETTLE_TIMEOUT_MS = 90_000;

// Two requests and two approvals, each behind a model call, with headroom
// that is deliberately not another retry.
const REQUEST_TEST_TIMEOUT_MS = 480_000;
const RUN_TEST_TIMEOUT_MS = 420_000;

const MESSAGE_PREFIX = "grant-walk-probe-";

interface WalkFleet {
  readonly id: string;
  readonly callsign: string;
}

// Shared by the serial chain: created once, granted once, run once.
let workspaceId = "";
let first: WalkFleet | null = null;
let second: WalkFleet | null = null;
let balanceBeforeNanos = 0n;

/** A fleet an earlier test in the chain made, or that test's own failure
 * restated so the follower does not report a missing id as a product defect. */
function granted(fleet: WalkFleet | null): WalkFleet {
  if (fleet === null) throw new Error("the request step did not complete; nothing to walk");
  return fleet;
}

/** The grant a fleet's request wrote, or a failure naming the agent whose
 *  request wrote none. */
function standing(grant: IntegrationGrant | null, callsign: string): IntegrationGrant {
  if (grant === null) throw new Error(`agent ${callsign} holds no integration grant`);
  return grant;
}

/** A fleet whose first message asks for the Linear handle; its run ends waiting on a card. */
async function requesting(name: string): Promise<WalkFleet> {
  const {fleet} = await observed(JOURNEY_LEG.install, () => createFixtureFleet(FIXTURE_KEY.regular, workspaceId,
    {name, message: accessMessage(GRANT_CREDENTIAL_NAME, uniqueTag())}));
  await observed(JOURNEY_LEG.install, () => expectFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id));
  return { id: fleet.id, callsign: fleetCallsign(fleet.id) };
}

/** One Approve on the fleet's own card moves its grant, and the run that asked resumes. */
async function approveRequest(page: Page, fleet: WalkFleet): Promise<void> {
  const gate = await pollFor(
    () => observed(JOURNEY_LEG.run, () => pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id)),
    REQUEST_TIMEOUT_MS,
  );
  if (gate === null) failWith(classifyIdleRequest(await anyRunnerLive(), fleet.callsign));
  const grant = standing(await connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id), fleet.callsign);
  expect(grant.status).toBe(GRANT_STATUS.pending);
  expect(grant.approved_at ?? null).toBeNull();
  await approveFleetRequest(page, workspaceId, fleet.id, gate, RENDER_TIMEOUT_MS);
  const approved = standing(await connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id), fleet.callsign);
  expect(approved.status).toBe(GRANT_STATUS.approved);
  expect(approved.approved_at ?? null).not.toBeNull();
  expect(await pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id)).toBeNull();
  assertPassed(classifyTerminalTurn(await awaitTerminalTurn(workspaceId, fleet.id)));
}

/** No card inside the budget: no runner says the environment, a runner says the fleet never asked. */
function classifyIdleRequest(runnerLive: boolean, callsign: string): FailedVerdict {
  return runnerLive
    ? { kind: VERDICT_KIND.product, leg: JOURNEY_LEG.run, detail: `agent ${callsign} never asked for ${GRANT_CREDENTIAL_NAME}` }
    : { kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.run, detail: "no runner is online to run the request" };
}

test.describe.serial("multi-fleet grant journey", () => {
  test.afterAll(async () => {
    if (!workspaceId) return;
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, SWEEP_PREFIX);
    await clientFor(FIXTURE_KEY.regular).delete(`/v1/workspaces/${workspaceId}/secrets/linear`);
  });

  test("two fleets each ask for access, and one approval each resumes its run", async ({ page }) => {
    test.setTimeout(REQUEST_TEST_TIMEOUT_MS);
    workspaceId = (await ensureSecondWorkspace(FIXTURE_KEY.regular, GRANT_WORKSPACE)).id;
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
    const alpha = await requesting(TEMPLATE_FIRST);
    const beta = await requesting(TEMPLATE_SECOND);
    expect(alpha.callsign, "two fleets must not share one callsign").not.toBe(beta.callsign);
    await approveRequest(page, alpha);
    first = alpha;
    await approveRequest(page, beta);
    second = beta;
  });

  test("the first delivery runs and runs, because the grant already stands", async ({
    page,
  }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = granted(first);
    await signInAs(page, FIXTURE_KEY.regular);

    // An online runner distinguishes a blocked grant from missing capacity.
    expect(
      await observed(JOURNEY_LEG.run, () => anyRunnerLive()),
      "no runner is online, so an idle delivery says nothing about the grant",
    ).toBe(true);

    const seen = await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    const held = await pollFor<RunLocation>(
      () => observed(JOURNEY_LEG.run, () => findRunFor(fleet.id, seen)),
      RUN_WINDOW_MS,
    );
    expect(
      held,
      "a delivery whose grant already stands must run, not park",
    ).not.toBeNull();

    // Every model call is its own turn. A continuation that asked again would
    // raise a card mid-run, which is the defect M202 closed — so the inbox is
    // read after the run, not only after the request.
    assertPassed(classifyTerminalTurn(await awaitTerminalTurn(workspaceId, fleet.id, seen)));
    expect(
      await observed(JOURNEY_LEG.observe, () =>
        pendingGateFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      `no turn of ${fleet.callsign}'s run may raise a ${KIND_INTEGRATION_GRANT} card`,
    ).toBeNull();

    // The grant the run holds is the one a person approved, still standing and
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
    const fleet = granted(second);
    const both = [granted(first).id, fleet.id];
    await signInAs(page, FIXTURE_KEY.regular);

    // The second fleet has its own explicit approval from the first test.
    standing(
      await observed(JOURNEY_LEG.observe, () =>
        connectorGrantFor(FIXTURE_KEY.regular, workspaceId, fleet.id),
      ),
      fleet.callsign,
    );

    const seen = await messageFleet(page, workspaceId, fleet.id, `${MESSAGE_PREFIX}${uniqueTag()}`);
    assertPassed(classifyTerminalTurn(await awaitTerminalTurn(workspaceId, fleet.id, seen)));
    await expectAnsweredOnScreen(page, workspaceId, fleet.id);

    // Both fleets spent something of their own, so the fall below is theirs and
    // not a parallel worker's.
    for (const fleetId of both) {
      const counters = await observed(JOURNEY_LEG.observe, () =>
        readFleetCounters(FIXTURE_KEY.regular, workspaceId, fleetId),
      );
      expect(BigInt(counters.events_processed), `fleet ${fleetId} processed no turn`).toBeGreaterThan(0n);
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
  test("the run's charge appears in Usage under its fleet and its models' names", async ({ page }) => {
    test.setTimeout(RUN_TEST_TIMEOUT_MS);
    const fleet = granted(second);
    const usage = await pollFor<FleetUsage>(
      () => observed(JOURNEY_LEG.observe, () => fleetUsage(FIXTURE_KEY.regular, fleet.id)),
      BILLING_SETTLE_TIMEOUT_MS,
    );
    expect(usage, "the settled report carries no charge for the fleet that just ran").not.toBeNull();
    const settled = usage as FleetUsage;
    const labels = await observed(JOURNEY_LEG.observe, () => catalogueLabels(FIXTURE_KEY.regular));
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/billing");
    // Narrow to the fleet the way a user does, through the Fleet filter.
    await page.getByRole("combobox", { name: "Fleet", exact: true }).click();
    await page.getByRole("option", { name: settled.fleetName, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`fleet=${fleet.id}`));
    await expect(page.locator('[aria-label="Period totals"]')).not.toContainText(ZERO_CHARGE);
    const byFleet = page.getByRole("table", { name: "Spending by fleet", exact: true });
    const row = byFleet.getByRole("row").filter({ hasText: settled.fleetName });
    for (const model of settled.models) {
      await expect(row.getByText(modelLabel(labels, model), { exact: true }), `model ${model} is named as the catalogue names it`).toBeVisible();
    }
    // Back to every fleet keeps the charge inside the account's month total.
    await page.getByRole("combobox", { name: "Fleet", exact: true }).click();
    await page.getByRole("option", { name: ALL_FLEETS_LABEL, exact: true }).click();
    await expect.poll(() => new URL(page.url()).searchParams.has("fleet")).toBe(false);
    await expect(page.locator('[aria-label="Period totals"]')).not.toContainText(ZERO_CHARGE);
  });
});
