import chief from "../../../../../tests/fixtures/fleets/chief-of-staff/definition.json";
import specialist from "../../../../../tests/fixtures/fleets/runner-link-watch/definition.json";
import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

// The fixture's one event: `chief-of-staff` steered `runner-link-watch`, and the turn waits for a runner.
const SENDER_LINE = "[from fleet chief-of-staff, fleet_id 0199a801-1000-7000-8000-0000000000c0; answer with fleet_steer to that id]";
const STEER_TEXT = "Run the hourly runner link check and send me its first lines.";

test("fleet_thread_renders_a_fleet_steer_row", async ({page, chatProbe}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?fleetSteer=1`);
  const turn = page.locator('[data-role="user"]');
  await expect(turn).toHaveCount(1);
  await expect(turn.getByTestId("fleet-message-sender")).toHaveText(`@${chief.config.name}`);
  await expect(turn).not.toContainText(SENDER_LINE);
  await expect(turn).toContainText(STEER_TEXT);
  // The pill a person's queued message shows, unchanged for a fleet's.
  const reply = page.locator('[data-role="assistant"]');
  await expect(reply.getByRole("status", {name: "Queued"})).toBeVisible();
  await expect(reply.getByRole("status", {name: "Working"})).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("the chief's conversation shows the requested check, the specialist's report and the chief's summary", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?fleetSteer=report`);
  await page.reload();
  await expect(page.getByText("Have runner-link-watch run its check and tell me the result.", {exact: true})).toBeVisible();
  await expect(page.getByText("I asked @runner-link-watch to check the connection. It will report back here.", {exact: true})).toBeVisible();
  const report = page.locator('[data-role="user"]').filter({hasText: "The runner link stayed connected"});
  await expect(report.getByTestId("fleet-message-sender")).toHaveText(`@${specialist.config.name}`);
  await expect(report).toContainText("No action needed. The runner link stayed connected during this check.");
  await expect(page.getByText("Runner Link Watch found no interruptions. No action is needed.", {exact: true})).toBeVisible();
  await expect(page.getByText(/answer with fleet_steer/)).toHaveCount(0);
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
    for (const [seconds, action, tool] of [[3, "Sent fleet message", "fleet_steer"], [2, "Read fleet activity", "fleet_activity"]] as const) {
      const group = page.getByRole("button", {name: `Tool activity: Worked for ${seconds}s · 1 tool`});
      await expect(group).toHaveAttribute("aria-expanded", "false");
      const work = group.locator("xpath=ancestor::*[@data-activity-group][1]/parent::*");
      await expect(work).toHaveCSS("border-top-width", "1px");
      await expect(work).toHaveCSS("border-bottom-width", "1px");
      await expect(group.locator(":scope > svg:last-child")).toHaveCSS("opacity", "1");
      await group.click();
      const step = page.getByRole("button", {name: `${action} Success`});
      await step.click();
      await expect(page.getByText(`Action · ${tool}`, {exact: true})).toBeVisible();
      const details = page.getByRole("region", {name: `${action} Success`});
      await expect(details.getByRole("heading", {name: "Output", exact: true})).toBeVisible();
      await step.click();
      await group.click();
    }
  }
  await page.screenshot({path: info.outputPath("chief-of-staff-report.png"), animations: "disabled"});
});
