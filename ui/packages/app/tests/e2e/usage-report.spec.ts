import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {BILLING_FLEET, BILLING_MODEL, BILLING_MODEL_NAME, billingReport, billingRowTotals, billingTotals} from "@/tests/helpers/billing-report";
import type {BillingReport} from "@/lib/api/billing_report";

function reportFor(query: URLSearchParams): BillingReport {
  const week = query.get("period") === "week";
  const month = (query.get("through_at") ?? "2026-11").slice(0, 7);
  const first = week ? 5 : 1;
  const daily = Array.from({length: 12 - first}, (_, index) => {
    const day = first + index;
    const charge = BigInt(day) * 1_000_000_000n / (query.has("fleet_id") ? 2n : 1n);
    return {day: `${month}-${String(day).padStart(2, "0")}`, totals: billingRowTotals({charged_nanos: charge.toString()})};
  });
  const totals = billingTotals({charged_nanos: daily.reduce((sum, item) => sum + BigInt(item.totals.charged_nanos), 0n).toString(), activity_count: daily.length});
  return billingReport({period: week ? "week" : "month", from_at: `${month}-${String(first).padStart(2, "0")}T00:00:00Z`, through_at: `${month}-11T12:00:00Z`,
    daily, totals, spending: {by_fleet: [{key: BILLING_FLEET, label: "Payment alerts", totals, models: [BILLING_MODEL, "retired-model"]}], by_model: []}});
}

test("Usage renders recorded daily bars and model sets while filters request complete reports", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => route.fulfill({json: reportFor(new URL(route.request().url()).searchParams)}));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?usage=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await expect(page.getByRole("combobox", {name: "Period", exact: true})).toContainText("November 2026");
    await expect(page.getByRole("combobox", {name: "Fleet", exact: true})).toContainText("All Fleets");
    for (const name of ["More filters", "Charge details"]) await expect(page.getByRole("button", {name, exact: true})).toHaveCount(0);
    await expect(page.getByRole("link", {name: "Add credits", exact: true})).toHaveCount(0);
    const chart = page.getByRole("list", {name: "Daily settled charges"});
    await expect(chart.getByRole("listitem")).toHaveCount(11);
    const bars = chart.getByRole("listitem").locator('[aria-hidden="true"]');
    const firstHeight = (await bars.first().boundingBox())?.height ?? 0;
    const lastHeight = (await bars.last().boundingBox())?.height ?? 0;
    expect(firstHeight).toBeGreaterThan(0);
    expect(lastHeight).toBeGreaterThan(firstHeight);

    await expect(page.getByText("Daily high $11.00", {exact: true})).toBeVisible();
    const fleets = page.getByRole("region", {name: "By fleet"});
    await expect(fleets.getByText(BILLING_MODEL_NAME, {exact: true})).toBeVisible();
    await expect(fleets.getByText("retired-model", {exact: true})).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`usage-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("combobox", {name: "Period", exact: true}).click();
    await page.getByRole("option", {name: "October 2026", exact: true}).click();
    await expect(page.getByRole("combobox", {name: "Period", exact: true})).toContainText("October 2026");
    await expect(chart.getByText("2026-10-01: $1.00", {exact: true})).toHaveCount(1);
    await page.getByRole("combobox", {name: "Period", exact: true}).click();
    await page.getByRole("option", {name: "Last 7 days", exact: true}).click();
    await expect(chart.getByRole("listitem")).toHaveCount(7);
    await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
    await expect(page.getByRole("option", {name: "Setup chat", exact: true})).toHaveCount(0);
    await expect(page.getByRole("option").first()).toHaveText("All Fleets");
    await page.getByRole("option", {name: "Payment alerts", exact: true}).click();
    await expect(page.getByText("Daily high $5.50", {exact: true})).toBeVisible();
    await expect(chart.getByText("2026-11-05: $2.50", {exact: true})).toHaveCount(1);
    await expect(page.locator('[aria-label="Period totals"]').getByText("$28.00", {exact: true})).toBeVisible();
    await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
    await page.getByRole("option", {name: "All Fleets", exact: true}).click();
    await expect(page.locator('[aria-label="Period totals"]').getByText("$56.00", {exact: true})).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test("Usage preserves exact large amounts and distinguishes zero from an unavailable report", async ({page, chatProbe}) => {
  let mode = "large";
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    if (mode === "unavailable") return route.fulfill({status: 403, json: {detail: "Fixture report refused", error_code: "UZ-AUTH-022"}});
    const totals = billingTotals({charged_nanos: mode === "large" ? "9007199254741049999" : "0"});
    const daily = mode === "oversized" ? Array.from({length: 32}, (_, index) => ({
      day: new Date(Date.UTC(2026, 9, index + 1)).toISOString().slice(0, 10), totals,
    })) : [{day: "2026-10-25", totals}];
    return route.fulfill({json: billingReport({totals, daily})});
  });
  await page.goto(`${chatProbe}?usage=1`);
  await expect(page.getByText("Daily high $9,007,199,254.741", {exact: true})).toBeVisible();
  mode = "zero";
  await page.reload();
  await expect(page.getByText("No settled charges for this period.", {exact: true})).toBeVisible();
  const zeroBars = page.getByRole("list", {name: "Daily settled charges"}).getByRole("listitem").locator('[aria-hidden="true"]');
  await expect(zeroBars).toHaveCount(1);
  expect(await zeroBars.first().evaluate(element => element.getBoundingClientRect().height)).toBe(0);
  mode = "oversized";
  await page.reload();
  await expect(page.getByText("Billing report unavailable", {exact: true})).toBeVisible();
  await expect(page.getByRole("figure", {name: "Daily spend"})).toHaveCount(0);
  mode = "unavailable";
  await page.reload();
  await expect(page.getByText("Billing report unavailable", {exact: true})).toBeVisible();
  await expect(page.getByText("No settled charges for this period.", {exact: true})).toHaveCount(0);
  await expect(page.getByRole("figure", {name: "Daily spend"})).toHaveCount(0);
});

for (const scenario of [
  {zone: "Asia/Kolkata", range: "Nov 1, 2026"},
  {zone: "America/New_York", range: "Oct 31 – Nov 1, 2026"},
]) test.describe(`viewer zone ${scenario.zone}`, () => {
  test.use({timezoneId: scenario.zone});
  test("keeps the API reporting month stable and formats its instants locally", async ({page, chatProbe}) => {
    const reads: URLSearchParams[] = [];
    await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
      const query = new URL(route.request().url()).searchParams; reads.push(query);
      const october = query.get("through_at") === "2026-10-31T23:59:59.999Z";
      return route.fulfill({json: billingReport({from_at: october ? "2026-10-01T00:00:00Z" : "2026-11-01T00:00:00Z",
        through_at: october ? "2026-10-31T23:59:59.999Z" : "2026-11-01T18:00:00Z",
        daily: [{day: october ? "2026-10-15" : "2026-11-01", totals: billingTotals()}],
      })});
    });
    await page.goto(`${chatProbe}?usage=1`);
    await expect(page.getByRole("combobox", {name: "Period", exact: true})).toContainText("November 2026");
    await expect(page.getByLabel("Period totals").locator("time")).toHaveText(scenario.range);
    await page.getByRole("combobox", {name: "Period", exact: true}).click();
    await page.getByRole("option", {name: "October 2026", exact: true}).click();
    await expect(page.getByRole("combobox", {name: "Period", exact: true})).toContainText("October 2026");
    expect(reads.at(-1)?.get("through_at")).toBe("2026-10-31T23:59:59.999Z");
    for(const param of ["month", "timezone", "activity_type", "fleet_id"]) expect(reads.at(-1)?.has(param)).toBe(false);
  });
});

test("old filter and paging URLs never reach the trimmed report API", async ({page, chatProbe}) => {
  const reads: URLSearchParams[] = [];
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    const query = new URL(route.request().url()).searchParams; reads.push(query);
    return route.fulfill({json: billingReport()});
  });
  await page.goto(`${chatProbe}?usage=1&model=${encodeURIComponent(BILLING_MODEL)}&limit=25&starting_after=old&activity=old&dc=old`);
  await expect(page.getByLabel("Period totals").getByText("$0.0001", {exact: true})).toBeVisible();
  expect(reads).toHaveLength(1);
  expect(Object.fromEntries(reads[0]!)).toEqual({period: "month"});
  await expect(page.getByRole("combobox", {name: "Model", exact: true})).toHaveCount(0);
  await expect(page.getByLabel("Active model filter")).toHaveCount(0);
  await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
  await page.getByRole("option", {name: "Payment alerts", exact: true}).click();
  await expect.poll(() => reads.length).toBe(2);
  expect(Object.fromEntries(reads[1]!)).toEqual({period: "month", fleet_id: BILLING_FLEET});
});

test("a slow shared label read keeps the Usage frame visible without another catalogue request", async ({page, chatProbe}, info) => {
  let reports = 0;
  const extraReads: string[] = [];
  page.on("request", request => {
    const path = new URL(request.url()).pathname;
    if (/\/backend\/v1\/(?:models|tenants\/me\/billing\/(?:charges|activities\/))/.test(path)) extraReads.push(path);
  });
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    reports += 1;
    return route.fulfill({json: billingReport()});
  });
  const started = performance.now();
  await page.goto(`${chatProbe}?usage=1&labelDelay=1200&activity=runtime:old`);
  await expect(page.getByRole("heading", {name: "Usage", exact: true})).toBeVisible();
  await expect(page.getByRole("status", {name: "Loading page"})).toBeVisible();
  await expect(page.getByLabel("Period totals")).toBeVisible();
  await expect(page.getByRole("figure", {name: "Daily spend"})).toBeVisible();
  const frameMs = performance.now() - started;
  await expect(page.getByRole("region", {name: "By fleet"}).getByText(BILLING_MODEL_NAME, {exact: true})).toBeVisible();
  const dataMs = performance.now() - started;
  expect(reports).toBe(1);
  expect(extraReads).toEqual([]);
  await info.attach("usage-label-performance", {body: JSON.stringify({kind: "local browser fixture", injectedLabelDelayMs: 1200,
    viewport: info.project.name, frameMs, dataMs, reportRequests: reports, catalogueOrChargeReads: extraReads.length}, null, 2), contentType: "application/json"});
});

test("a captured current month can return to live totals by selecting that month", async ({page, chatProbe}) => {
  const reads: URLSearchParams[] = [];
  await page.clock.setFixedTime(new Date("2026-11-15T12:00:00Z"));
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    const query = new URL(route.request().url()).searchParams; reads.push(query);
    const captured = query.has("through_at");
    return route.fulfill({json: billingReport({from_at: "2026-11-01T00:00:00Z",
      through_at: captured ? "2026-11-05T12:00:00Z" : "2026-11-15T12:00:00Z",
      totals: billingTotals({charged_nanos: captured ? "1000000000" : "2000000000"}),
    })});
  });
  await page.goto(`${chatProbe}?usage=1&through_at=2026-11-05T12%3A00%3A00Z`);
  await expect(page.getByRole("combobox", {name: "Period", exact: true})).toContainText("through");
  await page.getByRole("combobox", {name: "Period", exact: true}).click();
  await page.getByRole("option", {name: "November 2026", exact: true}).click();
  await expect(page.getByLabel("Period totals").getByText("$2.00", {exact: true})).toBeVisible();
  expect(reads).toHaveLength(2);
  expect(reads.at(-1)?.has("through_at")).toBe(false);
});
