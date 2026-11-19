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
  await expect(turn.getByTestId("fleet-message-sender")).toHaveText("Another fleet");
  await expect(turn).toContainText(SENDER_LINE);
  await expect(turn).toContainText(STEER_TEXT);
  // The pill a person's queued message shows, unchanged for a fleet's.
  const reply = page.locator('[data-role="assistant"]');
  await expect(reply.getByRole("status", {name: "Queued"})).toBeVisible();
  await expect(reply.getByRole("status", {name: "Working"})).toHaveCount(0);
  expect(errors).toEqual([]);
});
