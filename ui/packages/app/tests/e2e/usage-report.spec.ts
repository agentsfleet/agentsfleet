import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {BILLING_FLEET, BILLING_MODEL, BILLING_MODEL_NAME, billingReport, billingTotals} from "@/tests/helpers/billing-report";
import type {BillingReport} from "@/lib/api/billing_report";

function reportFor(query: URLSearchParams): BillingReport {
  const week = query.get("period") === "week";
  const first = week ? 5 : 1;
  const daily = Array.from({length: 12 - first}, (_, index) => {
    const day = first + index;
    const charge = BigInt(day) * 1_000_000_000n / (query.has("fleet_id") ? 2n : 1n);
    return {day: `2026-11-${String(day).padStart(2, "0")}`, totals: billingTotals({charged_nanos: charge.toString()})};
  });
  const totals = billingTotals({charged_nanos: daily.reduce((sum, item) => sum + BigInt(item.totals.charged_nanos), 0n).toString(), activity_count: daily.length});
  return billingReport({period: week ? "week" : "month", from_at: `2026-11-${String(first).padStart(2, "0")}T00:00:00Z`, through_at: "2026-11-11T12:00:00Z",
    daily, totals, spending: {by_fleet: [{key: BILLING_FLEET, label: "Payment alerts", totals, models: [BILLING_MODEL, "retired-model"]}], by_model: []},
    activities: {items: [], total: daily.length, next_cursor: "another-page"}});
}

test("Usage renders recorded daily bars and model sets while filters request complete reports", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => route.fulfill({json: reportFor(new URL(route.request().url()).searchParams)}));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?usage=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const chart = page.getByRole("list", {name: "Daily settled charges"});
    await expect(chart.getByRole("listitem")).toHaveCount(11);
    await expect(page.getByText("Daily high $11.00", {exact: true})).toBeVisible();
    const fleets = page.getByRole("region", {name: "By fleet"});
    await expect(fleets.getByText(BILLING_MODEL_NAME, {exact: true})).toBeVisible();
    await expect(fleets.getByText("retired-model", {exact: true})).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`usage-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("combobox", {name: "Period", exact: true}).click();
    await page.getByRole("option", {name: "Last 7 days", exact: true}).click();
    await expect(chart.getByRole("listitem")).toHaveCount(7);
    await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
    await page.getByRole("option", {name: "Payment alerts", exact: true}).click();
    await expect(page.getByText("Daily high $5.50", {exact: true})).toBeVisible();
    await expect(chart.getByText("2026-11-05: $2.50", {exact: true})).toHaveCount(1);
    await expect(page.locator('[aria-label="Period totals"]').getByText("$28.00", {exact: true})).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test("Usage preserves exact large amounts and distinguishes zero from an unavailable report", async ({page, chatProbe}) => {
  let mode = "large";
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    if (mode === "unavailable") return route.fulfill({status: 403, json: {detail: "Fixture report refused", error_code: "UZ-AUTH-022"}});
    const totals = billingTotals({charged_nanos: mode === "large" ? "9007199254741049999" : "0"});
    return route.fulfill({json: billingReport({totals, daily: [{day: "2026-10-25", totals}]})});
  });
  await page.goto(`${chatProbe}?usage=1`);
  await expect(page.getByText("Daily high $9,007,199,254.741", {exact: true})).toBeVisible();
  mode = "zero";
  await page.reload();
  await expect(page.getByText("No settled charges for this period.", {exact: true})).toBeVisible();
  mode = "unavailable";
  await page.reload();
  await expect(page.getByText("Billing report unavailable", {exact: true})).toBeVisible();
  await expect(page.getByText("No settled charges for this period.", {exact: true})).toHaveCount(0);
  await expect(page.getByRole("figure", {name: "Daily spend"})).toHaveCount(0);
});
