import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { workspaceHref } from "./nav";

/** The backend gate identifier pins the action to this saved fleet. */
export async function approveFleetRequest(page: Page, workspaceId: string, fleetId: string, gate: {gate_id: string; proposed_action: string}, timeoutMs: number): Promise<void> {
  await page.goto(`${workspaceHref(workspaceId, `fleets/${fleetId}`)}#fleet-approvals`);
  const request = page.locator(`[data-approval-gate="${gate.gate_id}"]`);
  await expect(request.getByText(gate.proposed_action, {exact: true})).toBeVisible({timeout: timeoutMs});
  await request.getByRole("button", {name: "Approve", exact: true}).click();
  await expect(request.getByText("Request approved.", {exact: true})).toBeVisible({timeout: timeoutMs});
  await expect(request.getByRole("button", {name: "Approve", exact: true})).toHaveCount(0);
}
