import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {billingActivity, billingReport} from "@/tests/helpers/billing-report";

for (const scenario of [
  {zone: "Asia/Kolkata", now: "2026-11-14T13:00:00Z", wake: "7:00 PM", day: "Nov 14, 2026"},
  {zone: "America/New_York", now: "2026-11-14T04:45:00Z", wake: "Nov 14, 12:15 AM", day: "Nov 13, 2026"},
]) test.describe(scenario.zone, () => {
  test.use({timezoneId: scenario.zone});

  test("wake-up and billing timestamps follow the reader's clock and calendar", async ({page, chatProbe}, info) => {
    const errors: string[] = [];page.on("pageerror", error => errors.push(error.message));
    await page.emulateMedia({reducedMotion: "reduce"});
    await page.clock.setFixedTime(new Date(scenario.now));
    await page.goto(`${chatProbe}?tasks=1&wakeups=1`);
    const footer = page.getByTestId("fleet-chat-footer");
    await expect(footer.locator("time")).toHaveText(scenario.wake);
    await expect(footer.getByText(/\bUTC\b/)).toHaveCount(0);
    await expect(page.getByRole("complementary", {name: "Summary"}).getByText("Next wake-up", {exact: true})).toHaveCount(0);
    await page.screenshot({path: info.outputPath("local-wake-up.png"), scale: "css", animations: "disabled"});

    await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => route.fulfill({json: billingReport({
      from_at: "2026-11-14T00:00:00Z", through_at: "2026-11-14T01:00:00Z",
      activities: {items: [billingActivity({recorded_at: "2026-11-14T00:30:00Z"})], total: 1, next_cursor: null},
    })}));
    await page.goto(`${chatProbe}?usage=1`);
    await expect(page.getByLabel("Period totals").locator("time")).toHaveText(scenario.day);
    await expect(page.getByRole("button", {name: "Charge details", exact: true})).toHaveCount(0);
    await expect(page.getByText(/All times UTC/)).toHaveCount(0);
    await page.screenshot({path: info.outputPath("local-usage.png"), scale: "css", animations: "disabled"});
    expect(errors).toEqual([]);
  });
});
