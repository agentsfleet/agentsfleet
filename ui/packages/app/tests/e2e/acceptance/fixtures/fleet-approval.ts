import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { workspaceHref } from "./nav";

/** What a person clicks on an access card, and the sentence the card answers with. */
export const CARD_DECISION = {
  approve: {button: "Approve", outcome: "Request approved."},
  deny: {button: "Deny", outcome: "Request denied."},
} as const;
export type CardDecision = (typeof CARD_DECISION)[keyof typeof CARD_DECISION];

type Card = {gate_id: string; proposed_action: string};

/** Opens the fleet's chat on its card. The backend gate identifier pins the card to this fleet's request. */
export async function openFleetRequest(page: Page, workspaceId: string, fleetId: string, gate: Card, timeoutMs: number): Promise<Locator> {
  await page.goto(`${workspaceHref(workspaceId, `fleets/${fleetId}`)}#fleet-approvals`);
  const request = page.locator(`[data-approval-gate="${gate.gate_id}"]`);
  await expect(request.getByText(gate.proposed_action, {exact: true})).toBeVisible({timeout: timeoutMs});
  return request;
}

/** One click on the fleet's own card, and the card saying what was decided. */
export async function decideFleetRequest(
  page: Page, workspaceId: string, fleetId: string, gate: Card, decision: CardDecision, timeoutMs: number,
): Promise<void> {
  const request = await openFleetRequest(page, workspaceId, fleetId, gate, timeoutMs);
  await request.getByRole("button", {name: decision.button, exact: true}).click();
  await expect(request.getByText(decision.outcome, {exact: true})).toBeVisible({timeout: timeoutMs});
  await expect(request.getByRole("button", {name: decision.button, exact: true})).toHaveCount(0);
}
