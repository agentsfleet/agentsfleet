/**
 * fleet-usage-billing.spec.ts — the credit a fleet's work spent, where a person
 * reads it.
 *
 * A fleet answers one real message; its own ledger rows and the tenant balance
 * show the work was billed, and Usage names the fleet and the models it ran
 * on. The fleet is the prime set's first-message fleet, made through the
 * shared helper: that a first message makes a fleet is the prime journey's to
 * prove, and this spec reads only what that message cost. Its display name is
 * its own: Usage's Fleet filter lists every fleet the tenant was charged for
 * this month by name, and other journeys make the same fixture in this tenant.
 */
import { expect, test } from "@playwright/test";
import { ALL_FLEETS_LABEL } from "@/app/(dashboard)/settings/billing/lib/report-format";
import { modelLabel } from "@/lib/model-labels";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, failWith, JOURNEY_LEG, VERDICT_KIND } from "./fixtures/execution";
import { chargedToFleets, readTenantBilling } from "./fixtures/grants";
import { observed, pollFor } from "./fixtures/observation";
import { createPrimeFleet, PRIME_FLEET, primeFleet, type PrimedFleet } from "./fixtures/prime-fleets";
import { threadOf } from "./fixtures/prime-walk";
import { getDefaultWorkspaceId, readFleetCounters } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { catalogueLabels, fleetUsage, type FleetUsage } from "./fixtures/usage";

const fixture = primeFleet(PRIME_FLEET.morningRooster);
const USAGE_FLEET_NAME = "usage-billing";
// A settled charge can be a fraction of a cent ("$0.0001"), so only an exact zero counts as none.
const ZERO_CHARGE = /\$0\.00(?!\d)/;
// The fleet's first turn on a runner, then the ledger behind it.
const RUN_TEST_TIMEOUT_MS = 420_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BILLING_SETTLE_TIMEOUT_MS = 90_000;

test.describe.serial("fleet usage and billing", () => {
  test.setTimeout(RUN_TEST_TIMEOUT_MS);
  let workspaceId = "";
  let fleet: PrimedFleet | null = null;

  /** The fleet the first test ran, or that test's own failure restated. */
  const billed = (): PrimedFleet => {
    if (fleet === null) throw new Error("no fleet ran a message; nothing was billed");
    return fleet;
  };

  test.afterAll(async () => {
    if (fleet) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleet.id);
  });

  test("a fleet's answered message is charged to it, and the tenant credit balance falls", async () => {
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    // Read before anything runs: the fall below is measured from this figure.
    const billing = await observed(JOURNEY_LEG.install, () => readTenantBilling(FIXTURE_KEY.regular));
    if (billing.is_exhausted) {
      failWith({kind: VERDICT_KIND.environment, leg: JOURNEY_LEG.install, detail: "the fixture tenant's credit is exhausted; no run can be billed"});
    }
    const balanceBeforeNanos = BigInt(billing.balance_nanos);
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(FIXTURE_KEY.regular, workspaceId, {...fixture, name: USAGE_FLEET_NAME}));
    const first = (await threadOf(workspaceId, billed().id)).find((turn) => turn.turn_id === billed().turnId);
    if (first === undefined) throw new Error(`fleet ${billed().id} holds no first turn`);
    assertPassed(classifyTerminalTurn(first));

    // The fleet spent something of its own, so the fall below is its and not a parallel worker's.
    const counters = await observed(JOURNEY_LEG.observe, () => readFleetCounters(FIXTURE_KEY.regular, workspaceId, billed().id));
    expect(BigInt(counters.events_processed), "the fleet processed no turn").toBeGreaterThan(0n);
    await expect.poll(() => observed(JOURNEY_LEG.observe, () => chargedToFleets(FIXTURE_KEY.regular, [billed().id])),
      {message: "the usage ledger carries no charge for the fleet", timeout: BILLING_SETTLE_TIMEOUT_MS}).toBeGreaterThan(0n);
    // Nanos, not the rendered card: the balance card rounds to cents, and a run
    // costing a tenth of one would leave the figure unchanged.
    await expect.poll(async () => BigInt((await observed(JOURNEY_LEG.observe, () => readTenantBilling(FIXTURE_KEY.regular))).balance_nanos),
      {timeout: BILLING_SETTLE_TIMEOUT_MS}).toBeLessThan(balanceBeforeNanos);
  });

  test("the run's charge appears in Usage under its fleet and its models' names", async ({ page }) => {
    const usage = await pollFor<FleetUsage>(
      () => observed(JOURNEY_LEG.observe, () => fleetUsage(FIXTURE_KEY.regular, billed().id)),
      BILLING_SETTLE_TIMEOUT_MS,
    );
    if (usage === null) throw new Error("the settled report carries no charge for the fleet that just ran");
    const labels = await observed(JOURNEY_LEG.observe, () => catalogueLabels(FIXTURE_KEY.regular));
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/billing");
    // Narrow to the fleet the way a user does, through the Fleet filter.
    await page.getByRole("combobox", { name: "Fleet", exact: true }).click();
    await page.getByRole("option", { name: usage.fleetName, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`fleet=${billed().id}`));
    await expect(page.locator('[aria-label="Period totals"]')).not.toContainText(ZERO_CHARGE);
    const byFleet = page.getByRole("table", { name: "Spending by fleet", exact: true });
    const row = byFleet.getByRole("row").filter({ hasText: usage.fleetName });
    for (const model of usage.models) {
      await expect(row.getByText(modelLabel(labels, model), { exact: true }), `model ${model} is named as the catalogue names it`).toBeVisible();
    }
    // Back to every fleet keeps the charge inside the account's month total.
    await page.getByRole("combobox", { name: "Fleet", exact: true }).click();
    await page.getByRole("option", { name: ALL_FLEETS_LABEL, exact: true }).click();
    await expect.poll(() => new URL(page.url()).searchParams.has("fleet")).toBe(false);
    await expect(page.locator('[aria-label="Period totals"]')).not.toContainText(ZERO_CHARGE);
  });
});
