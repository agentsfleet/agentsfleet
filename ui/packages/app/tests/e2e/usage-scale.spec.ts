import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {BILLING_MODEL, billingReport, billingRowTotals, billingTotals} from "@/tests/helpers/billing-report";
import type {BillingFleetBreakdown} from "@/lib/api/billing_report";

const DAYS = 31;
const DOLLAR = 1_000_000_000n;
const SETUP_DAILY = DOLLAR / 2n;

function fleets(count: number) {
  return Array.from({length: count}, (_, index) => ({
    id: `01924f4e-0000-7000-8000-${String(index + 1).padStart(12, "0")}`,
    name: `Support queue ${index + 1}`, daily: BigInt(index + 1) * DOLLAR,
  }));
}

function amounts(daily: bigint, activities: number, days: number) {
  const lines = activities * days;
  return billingTotals({charged_nanos: (daily * BigInt(days)).toString(), activity_count: activities,
    input_tokens: lines * 40, output_tokens: lines * 10, runtime_ms: lines * 1_000, line_count: lines});
}

function rowAmounts({activity_count: _count, ...row}: ReturnType<typeof amounts>) { return billingRowTotals(row); }

function reportFor(count: number, selected: string | null) {
  const choices = fleets(count), visible = selected ? choices.filter(fleet => fleet.id === selected) : choices;
  const setup = selected === null;
  const daily = visible.reduce((sum, fleet) => sum + fleet.daily, setup ? SETUP_DAILY : 0n);
  const activityCount = visible.length + Number(setup);
  const totals = amounts(daily, activityCount, DAYS);
  const rows: BillingFleetBreakdown[] = visible.map(fleet => ({key: fleet.id, label: fleet.name, models: [BILLING_MODEL], totals: amounts(fleet.daily, 1, DAYS)}));
  if (setup) {
    rows.push({key: null, label: "Setup chats", models: [BILLING_MODEL], totals: amounts(SETUP_DAILY, 1, DAYS)});
  }
  return billingReport({from_at: "2026-10-01T00:00:00Z", through_at: "2026-10-31T23:59:59.999Z", totals,
    daily: Array.from({length: DAYS}, (_, index) => ({day: `2026-10-${String(index + 1).padStart(2, "0")}`, totals: rowAmounts(amounts(daily, activityCount, 1))})),
    filters: {fleets: [{id: null, name: null}, ...choices.map(({id, name}) => ({id, name}))], models: [BILLING_MODEL]},
    spending: {by_fleet: rows, by_model: [{key: BILLING_MODEL, label: null, totals: rowAmounts(totals)}]},
  });
}

for (const count of [1, 100]) test(`${count} fleets keep daily bars bounded and filters request one complete report`, async ({page, chatProbe}, info) => {
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  const requests: URLSearchParams[] = [], payloadBytes: number[] = [];
  const extraReads: string[] = [];
  page.on("request", request => {
    const url = new URL(request.url());
    if (/\/backend\/v1\/(?:models|tenants\/me\/billing\/(?:charges|activities\/))/.test(url.pathname)) extraReads.push(url.pathname);
  });
  await page.route("**/backend/v1/tenants/me/billing/reports/current?*", route => {
    const query = new URL(route.request().url()).searchParams;
    requests.push(query);
    const body = JSON.stringify(reportFor(count, query.get("fleet_id")));
    payloadBytes.push(new TextEncoder().encode(body).byteLength);
    return route.fulfill({contentType: "application/json", body});
  });
  const results = [];
  for (const theme of ["dark", "light"]) {
    const before = requests.length;
    const started = performance.now();
    await page.goto(`${chatProbe}?usage=1&period=month&through_at=2026-10-31T23%3A59%3A59.999Z`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const totals = page.getByLabel("Period totals");
    const allAmount = count === 1 ? "$46.50" : "$156,565.50";
    const fleetAmount = count === 1 ? "$31.00" : "$3,100.00";
    await expect(totals.getByText(allAmount, {exact: true})).toBeVisible();
    const firstRenderMs = performance.now() - started;
    const chart = page.getByRole("list", {name: "Daily settled charges"});
    await expect(chart.getByRole("listitem")).toHaveCount(DAYS);
    const chartBox = await chart.boundingBox();
    expect(chartBox?.height).toBeLessThanOrEqual(102);
    expect(chartBox?.width).toBeLessThanOrEqual(page.viewportSize()!.width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.getByRole("region", {name: "By fleet"}).getByText("Setup chats", {exact: true})).toBeVisible();
    const rows = page.getByRole("table", {name: "Spending by fleet"}).locator("tbody > tr");
    await expect(rows).toHaveCount(count + 1);
    await expect(page.getByText(`Daily high ${count === 1 ? "$1.50" : "$5,050.50"}`, {exact: true})).toBeVisible();

    const opened = performance.now();
    await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
    await expect(page.getByRole("option")).toHaveCount(count + 1);
    await expect(page.getByRole("option").first()).toHaveText("All Fleets");
    const listbox = await page.getByRole("listbox").boundingBox();
    expect(listbox?.y).toBeGreaterThanOrEqual(0);
    expect((listbox?.y ?? 0) + (listbox?.height ?? Infinity)).toBeLessThanOrEqual(page.viewportSize()!.height);
    const dropdownOpenMs = performance.now() - opened;
    const switched = performance.now();
    await page.getByRole("option", {name: `Support queue ${count}`, exact: true}).click();
    await expect(totals.getByText(fleetAmount, {exact: true})).toBeVisible();
    const fleetSwitchMs = performance.now() - switched;
    await expect(chart.getByRole("listitem")).toHaveCount(DAYS);
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText(`Support queue ${count}`);
    await expect(page.getByText(`Daily high ${count === 1 ? "$1.00" : "$100.00"}`, {exact: true})).toBeVisible();
    expect(requests.at(-1)?.get("fleet_id")).toBe(fleets(count).at(-1)?.id);
    await expect(page.getByRole("region", {name: "By fleet"}).getByText("Setup chats", {exact: true})).toHaveCount(0);

    const allStarted = performance.now();
    await page.getByRole("combobox", {name: "Fleet", exact: true}).click();
    await page.getByRole("option", {name: "All Fleets", exact: true}).click();
    await expect(totals.getByText(allAmount, {exact: true})).toBeVisible();
    await expect(rows).toHaveCount(count + 1);
    const allFleetsMs = performance.now() - allStarted;
    expect(requests).toHaveLength(before + 3);
    expect(requests.at(-1)?.has("fleet_id")).toBe(false);
    expect(requests.at(-1)?.has("activity_type")).toBe(false);
    expect(extraReads).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`usage-${count}-${theme}.png`), animations: "disabled", scale: "css"});
    results.push({fleets: count, theme, viewport: info.project.name, firstRenderMs, dropdownOpenMs, fleetSwitchMs, allFleetsMs,
      reportRequests: requests.length - before, catalogueOrChargeReads: extraReads.length, bars: DAYS,
      chartHeightPx: chartBox?.height, dropdownHeightPx: listbox?.height, reportBytes: payloadBytes[before]});
  }
  await info.attach("usage-performance", {body: JSON.stringify({kind: "local browser fixture; HTTP responses injected without delay", results}, null, 2), contentType: "application/json"});
});
