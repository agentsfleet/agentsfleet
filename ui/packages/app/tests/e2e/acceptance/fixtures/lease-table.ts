/**
 * lease-table.ts — a runner's Leases table as an operator reads it.
 *
 * The table's Fleet cell names the agent by callsign (`LeaseTable.tsx`, through
 * `agentDisplayName`) and carries the fleet id on its `title`; the fleet's given
 * name is not in this table at all — it is Review lease's to show. A walk
 * therefore locates rows by the id: it is the one per-row key that cannot be
 * shared, where a callsign is drawn from a small space and the DEV runner is
 * shared by every spec's fleets at once.
 */
import { expect, type Locator, type Page } from "@playwright/test";
import { LEASES_TABLE_LABEL } from "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy";
import { agentDisplayName } from "@/lib/fleets/agent-label";

/** The leases table itself, by its caption. */
export function leasesTable(page: Page): Locator {
  return page.getByRole("table", { name: LEASES_TABLE_LABEL });
}

/**
 * The Fleet cell of one lease row, by the fleet id its title carries. Scoped to
 * a row on purpose: the filter bar's fleet chip carries the same id as its own
 * title, so a page-wide lookup would match two elements under a fleet filter.
 */
export function agentCellFor(row: Locator, fleetId: string): Locator {
  return row.getByTitle(fleetId, { exact: true });
}

/** Every lease row held by one fleet. */
export function leaseRowsFor(page: Page, fleetId: string): Locator {
  // `has` matches from the row inward, so the page-rooted locator cannot reach
  // the filter chip; it only names the attribute to look for.
  return leasesTable(page)
    .getByRole("row")
    .filter({ has: page.getByTitle(fleetId, { exact: true }) });
}

/** Assert the row names its agent the way every other agent column does. */
export async function expectAgentLabel(row: Locator, fleetId: string): Promise<void> {
  await expect(agentCellFor(row, fleetId)).toHaveText(agentDisplayName(fleetId));
}
