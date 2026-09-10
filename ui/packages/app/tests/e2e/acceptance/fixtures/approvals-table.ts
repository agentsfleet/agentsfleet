/**
 * approvals-table.ts — the Approvals inbox as an operator reads it.
 *
 * Every string this file locates by is imported from the page's own copy module
 * rather than re-spelled, so a rename lands as a compile error here instead of
 * as a mystery timeout (`approvals/copy.ts` is a plain constants module — no
 * "use client", no React — so a Playwright process can import it).
 *
 * The one thing NOT imported is the request headline. That sentence is composed
 * by the daemon (`afd_approval::request::PROPOSED_ACTION_PREFIX`) and has no
 * TypeScript sibling, so the walk reads it off the card over the API and uses
 * the value the server actually wrote. A copy of the prefix here would be a
 * second spelling of a string only one side owns.
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  AGENT_PREFIX,
  APPROVALS_TABLE_CAPTION,
  APPROVE_LABEL,
  STATUS_LABEL,
} from "@/app/(dashboard)/w/[workspaceId]/approvals/copy";
import { workspaceHref } from "./nav";

/** The region that holds every gate, in every state. Renamed from "Pending
 * approval gates" when the table stopped being pending-only. */
export const APPROVAL_GATES_REGION_LABEL = "Approval gates";

/** The inbox, filtered server-side to one fleet. The filter is what keeps the
 * row lookup deterministic: the fixture workspace is shared, and an unfiltered
 * page is capped at its own page size. */
export function approvalsHrefForFleet(workspaceId: string, fleetId: string): string {
  return `${workspaceHref(workspaceId, "approvals")}?fleetId=${encodeURIComponent(fleetId)}`;
}

/** The gates table itself, by its caption. */
export function gatesTable(page: Page): Locator {
  return page.getByRole("table", { name: APPROVALS_TABLE_CAPTION });
}

/**
 * The row for one agent, found by the callsign its Fleet cell prints.
 *
 * The callsign, not the fleet name: it is what `AgentLabel` renders on every
 * surface that names a fleet, so this is the same identifier the wall tile and
 * the billing rows carry — and it is derived from the fleet id, so it cannot be
 * confused with a server-suffixed name.
 */
export function rowForAgent(page: Page, callsign: string): Locator {
  return gatesTable(page)
    .getByRole("row")
    .filter({ hasText: `${AGENT_PREFIX} ${callsign}` });
}

/** Assert the row reads Pending — the state that still asks for something. */
export async function expectPending(row: Locator, timeoutMs: number): Promise<void> {
  await expect(row.getByText(STATUS_LABEL.pending, { exact: true })).toBeVisible({
    timeout: timeoutMs,
  });
}

/**
 * Answer the card, and wait for the row to come back under its new status.
 *
 * The row does not leave the table on approve — it is re-read and re-rendered —
 * so the assertion is the state CHANGE on one row, which is what an operator
 * watches happen.
 */
export async function approveRow(
  row: Locator,
  proposedAction: string,
  timeoutMs: number,
): Promise<void> {
  await row.getByRole("button", { name: `${APPROVE_LABEL}: ${proposedAction}` }).click();
  await expect(row.getByText(STATUS_LABEL.approved, { exact: true })).toBeVisible({
    timeout: timeoutMs,
  });
}

/**
 * Assert the DECIDED column names the person who decided.
 *
 * By `title`, not by text: the cell prints this deployment's own name for the
 * subject when it has one and the shortened subject when it does not, so the
 * only assertion true in both cases is that the cell carries the subject the
 * daemon recorded — which is exactly the claim "the column names the decider".
 */
export async function expectDecidedBy(
  row: Locator,
  resolvedBy: string,
  timeoutMs: number,
): Promise<void> {
  await expect(row.getByTitle(resolvedBy)).toBeVisible({ timeout: timeoutMs });
}
