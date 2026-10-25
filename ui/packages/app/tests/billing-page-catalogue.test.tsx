import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { mockAuthOnce } from "./helpers/dashboard-mocks";
import { BILLING_AT, BILLING_CATALOGUE, BILLING_MODEL, BILLING_MODEL_NAME, BILLING_SELECTION, BILLING_WORK, billingActivity, billingCharge, billingReport } from "./helpers/billing-report";
import type { BillingSearch } from "@/app/(dashboard)/settings/billing/lib/report-query";
import { activityKey } from "@/app/(dashboard)/settings/billing/lib/report-query";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing_report";

vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
import BillingSettingsPage from "@/app/(dashboard)/settings/billing/page";

type Backend = { catalogue?: unknown; catalogueStatus?: number; reportStatus?: number; report?: unknown; detailStatus?: number; detail?: unknown };
const requests: URL[] = [];

function backend(options: Backend = {}) {
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
    const url = new URL(typeof input === "string" || input instanceof URL ? input : input.url, "https://example.test");
    requests.push(url);
    if (url.pathname.endsWith("/models")) return Response.json(options.catalogue ?? BILLING_CATALOGUE, { status: options.catalogueStatus ?? 200 });
    if (url.pathname.endsWith("/reports/current")) return Response.json(options.report ?? billingReport(), { status: options.reportStatus ?? 200 });
    if (url.pathname.endsWith("/charges")) return Response.json(options.detail ?? { items: [billingCharge()], total: null, next_cursor: null }, { status: options.detailStatus ?? 200 });
    throw new Error(`Unexpected test request: ${url.pathname}`);
  }));
}

async function markup(query?: BillingSearch): Promise<string> {
  const page = await BillingSettingsPage(query ? { searchParams: Promise.resolve(query) } : {});
  return renderToStaticMarkup(<TooltipProvider>{page}</TooltipProvider>);
}

beforeEach(() => { mockAuthOnce({ token: "fixture-token" }); requests.length = 0; });
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

describe("served billing page composition", () => {
  it("enriches exact ledger identity and provides the existing Add credits action", async () => {
    backend();
    const html = await markup();
    expect(html).toContain(BILLING_MODEL_NAME);
    expect(html).toContain("Payment alerts");
    expect(html).toContain("Charged this period");
    expect(html).toContain("$0.0001");
    expect(html).toContain("Add credits");
    expect(html).toContain("mailto:agentsfleet@agentmail.to");
    expect(html).not.toContain("Available credits");
    expect(requests.some(url => url.pathname.endsWith("/charges"))).toBe(false);
  });

  it.each([{ catalogueStatus: 403 }, { catalogue: { models: [{ id: BILLING_MODEL }] } }])("keeps ledger data when catalogue enrichment fails: %j", async options => {
    backend(options);
    const html = await markup();
    expect(html).toContain(BILLING_MODEL);
    expect(html).toContain("Charged this period");
    expect(html).toContain("$0.0001");
    expect(html).not.toContain(BILLING_MODEL_NAME);
  });

  it.each([{ reportStatus: 403 }, { report: { totals: {} } }])("a refused or malformed report shows recovery without claiming zero: %j", async options => {
    backend(options);
    const html = await markup();
    expect(html).toContain("Billing report unavailable");
    expect(html).toContain("Retry report");
    expect(html).toContain("Reset filters");
    expect(html).not.toContain("No settled activity");
  });

  it("threads exact filters, cutoff and page-size-bound continuation to the report", async () => {
    backend();
    await markup({ period: "today", model: BILLING_MODEL, fleet: "setup", through_at: BILLING_AT, c: ["previous", "current"], cps: "50", ps: "50" });
    const params = requests.find(url => url.pathname.endsWith("/reports/current"))?.searchParams;
    expect(params?.get("period")).toBe("today");
    expect(params?.get("activity_type")).toBe("setup");
    expect(params?.get("model")).toBe(BILLING_MODEL);
    expect(params?.get("through_at")).toBe(BILLING_AT);
    expect(params?.get("starting_after")).toBe("current");
    expect(params?.get("limit")).toBe("50");
  });

  it("loads only an expanded activity in the visible page and uses the report cutoff", async () => {
    backend();
    const html = await markup({ activity: BILLING_SELECTION, dc: "detail-page" });
    const url = requests.find(url => url.pathname.endsWith("/charges"));
    expect(url?.pathname).toContain(BILLING_WORK);
    expect(url?.searchParams.get("through_at")).toBe(BILLING_AT);
    expect(url?.searchParams.get("activity_type")).toBe("runtime");
    expect(url?.searchParams.get("starting_after")).toBe("detail-page");
    expect(html).toContain("Page 2");
  });

  it("keeps runtime and setup disclosure identities separate when their identifiers overlap", async () => {
    const setup = billingActivity({ activity_type: BILLING_ACTIVITY_SETUP, fleet_id: null, fleet_name: null });
    backend({ report: billingReport({ activities: { items: [billingActivity(), setup], total: 2, next_cursor: null } }) });
    const html = await markup({ activity: activityKey(setup) });
    const details = requests.filter(url => url.pathname.endsWith("/charges"));
    expect(details).toHaveLength(1);
    expect(details[0]?.pathname).toContain(BILLING_WORK);
    expect(details[0]?.searchParams.get("activity_type")).toBe(BILLING_ACTIVITY_SETUP);
    expect(html).toContain("Activity charge details");
    expect(html.replaceAll(/<!--.*?-->/g, "")).toContain("Setup chat charge details");
    expect(html).toContain("Total charged");
  });

  it.each([{ detailStatus: 403 }, { detail: { items: [{ charged_nanos: "broken" }] } }])("detail failure retains period totals and provides retry: %j", async options => {
    backend(options);
    const html = await markup({ activity: BILLING_SELECTION });
    expect(html).toContain("Charges unavailable");
    expect(html).toContain("Retry charges");
    expect(html).toContain("Charged this period");
  });

  it("ignores a disclosure naming a work item outside the visible page", async () => {
    backend();
    await markup({ activity: "foreign-work" });
    expect(requests.some(url => url.pathname.endsWith("/charges"))).toBe(false);
  });
});
