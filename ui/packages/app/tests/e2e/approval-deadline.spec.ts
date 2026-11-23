import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

// A passed deadline closes nothing: the daemon reports the unanswered gate as
// `timed_out`, and the conversation's card still takes either decision. The
// route stands in for the daemon's approval list.
const APPROVALS_PATH = `/backend/v1/workspaces/${WORKSPACE}/approvals`;
const HOUR_MS = 3_600_000;

test("an_expired_gate_card_still_offers_approve_and_deny", async ({page, chatProbe}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize({width: 1480, height: 1000});
  const raised = Date.now() - 3 * HOUR_MS;
  const gate = {gate_id: "late-release-approval", fleet_id: FLEET_ID, fleet_name: "Release watch", workspace_id: WORKSPACE,
    action_id: "publish-runner-image", tool_name: "registry", action_name: "publish", gate_type: "destructive_action",
    proposed_action: "Publish runner image", evidence: {}, blast_radius: "Production registry · Release tag", status: "timed_out",
    detail: "", created_at: new Date(raised).toISOString(), expires_at: new Date(raised + HOUR_MS).toISOString(),
    resolved_at: null, resolved_by: "", resolved_by_name: ""};
  await page.route(url => url.pathname === APPROVALS_PATH, route => route.fulfill({json: {items: [gate], next_cursor: null}}));
  await page.goto(`${chatProbe}?visual=1&state=completed&approvalRoute=1`);
  const card = page.locator(`[data-approval-gate="${gate.gate_id}"]`);
  await expect(card.getByText(gate.proposed_action, {exact: true})).toBeVisible();
  await expect(card.getByText("Deadline passed", {exact: true})).toBeVisible();
  await expect(card.getByRole("button", {name: "Approve", exact: true})).toBeEnabled();
  await expect(card.getByRole("button", {name: "Deny", exact: true})).toBeEnabled();
  expect(errors).toEqual([]);
});
