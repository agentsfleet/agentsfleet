import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToReadableStream } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ModelLabelsProvider } from "@/components/domain/models/ModelLabels";
import { mockAuthOnce } from "./helpers/dashboard-mocks";
import { BILLING_AT, BILLING_MODEL, BILLING_MODEL_NAME, BILLING_SELECTION, BILLING_WORK, billingActivity, billingCharge, billingReport } from "./helpers/billing-report";
import type { BillingSearch } from "@/app/(dashboard)/settings/billing/lib/report-query";
import { activityKey } from "@/app/(dashboard)/settings/billing/lib/report-query";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing_report";

vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
import BillingSettingsPage from "@/app/(dashboard)/settings/billing/page";
import { UsageReport } from "@/app/(dashboard)/settings/billing/components/UsageReport";

type Backend = { reportStatus?: number; report?: unknown; reportGate?: Promise<void>; reportFailure?: Error; detailStatus?: number; detail?: unknown };
const requests: URL[] = [];
const LABELS = { [BILLING_MODEL]: BILLING_MODEL_NAME };

function backend(options: Backend = {}) {
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request) => {
    const url = new URL(typeof input === "string" || input instanceof URL ? input : input.url, "https://example.test");
    requests.push(url);
    if (url.pathname.endsWith("/reports/current")) {
      await options.reportGate;
      if (options.reportFailure) throw options.reportFailure;
      return Response.json(options.report ?? billingReport(), { status: options.reportStatus ?? 200 });
    }
    if (url.pathname.endsWith("/charges")) return Response.json(options.detail ?? { items: [billingCharge()], total: null, next_cursor: null }, { status: options.detailStatus ?? 200 });
    throw new Error(`Unexpected test request: ${url.pathname}`);
  }));
}

const decoder = new TextDecoder();

/** The streamed data region after its reads settle, inside the layout's label store. */
async function markup(query: BillingSearch = {}, labels: Record<string, string> = LABELS): Promise<string> {
  const region = await UsageReport({ token: "fixture-token", query });
  const stream = await renderToReadableStream(<TooltipProvider><ModelLabelsProvider labels={labels}>{region}</ModelLabelsProvider></TooltipProvider>);
  await stream.allReady;
  return new Response(stream).text().then(html => html.replaceAll(/<template[^>]*><\/template>|<!--\$\??-->|<!--\/\$-->/g, ""));
}

const reads = (suffix: string) => requests.filter(url => url.pathname.endsWith(suffix));

beforeEach(() => { mockAuthOnce({ token: "fixture-token" }); requests.length = 0; });
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

describe("served billing page composition", () => {
  it("usage_first_load_has_one_report_request: the first load reads one report and no catalogue or charges", async () => {
    backend();
    const html = await markup();
    expect(html).toContain("Settled charges");
    expect(html).toContain("Payment alerts");
    expect(html).toContain(BILLING_MODEL_NAME);
    expect(requests).toHaveLength(1);
  });

  it("names models from the label store, and shows identifiers it does not name without zeroing usage", async () => {
    const report = billingReport();
    backend({ report: billingReport({ spending: { ...report.spending,
      by_fleet: [{ key: report.spending.by_fleet[0]!.key, label: "Payment alerts", models: [BILLING_MODEL, "retired-model", "constructor"], totals: report.totals }] } }) });
    const html = await markup();
    expect(html).toContain(BILLING_MODEL_NAME);
    expect(html).toContain("retired-model");
    expect(html).toContain("constructor");
    expect(html).not.toContain("function Object");
    requests.length = 0;
    const unlabelled = await markup({}, {});
    expect(unlabelled).toContain(BILLING_MODEL);
    expect(unlabelled).not.toContain(BILLING_MODEL_NAME);
    expect(unlabelled).toContain("$0.0001");
    expect(requests).toHaveLength(1);
  });

  it("usage_shell_streams_before_report: the frame renders while the report is pending", async () => {
    let release = () => {};
    backend({ reportGate: new Promise<void>(resolve => { release = resolve; }) });
    const page = await BillingSettingsPage({});
    const stream = await renderToReadableStream(<TooltipProvider><ModelLabelsProvider labels={LABELS}>{page}</ModelLabelsProvider></TooltipProvider>);
    const streamed = stream.getReader();
    const shell = decoder.decode((await streamed.read()).value);
    expect(shell).toContain("Usage");
    expect(shell).toContain("Loading page");
    expect(shell).not.toContain("Settled charges");
    expect(reads("/reports/current")).toHaveLength(1);
    release();
    let rest = "";
    for (let chunk = await streamed.read(); !chunk.done; chunk = await streamed.read()) rest += decoder.decode(chunk.value);
    expect(rest).toContain("Settled charges");
  });

  it.each([{ reportStatus: 403 }, { report: { totals: {} } }, { reportFailure: new TypeError("fetch failed") }])("a refused, malformed or unreachable report shows recovery without claiming zero: %j", async options => {
    backend(options);
    const html = await markup({ activity: BILLING_SELECTION });
    expect(html).toContain("Billing report unavailable");
    expect(html).toContain("Retry report");
    expect(html).toContain("Reset filters");
    expect(html).not.toContain("No settled activity");
    expect(reads("/charges")).toHaveLength(0);
  });

  it("threads exact filters, cutoff and page-size-bound continuation to the report", async () => {
    backend();
    await markup({ period: "today", model: BILLING_MODEL, fleet: "setup", through_at: BILLING_AT, c: ["previous", "current"], cps: "50", ps: "50" });
    const params = reads("/reports/current")[0]?.searchParams;
    expect(params?.get("period")).toBe("today");
    expect(params?.get("activity_type")).toBe("setup");
    expect(params?.get("model")).toBe(BILLING_MODEL);
    expect(params?.get("through_at")).toBe(BILLING_AT);
    expect(params?.get("starting_after")).toBe("current");
    expect(params?.get("limit")).toBe("50");
  });

  it("reads charges only for an opened activity in the visible page, at the report's cutoff", async () => {
    backend();
    const html = await markup({ activity: BILLING_SELECTION, dc: "detail-page" });
    const url = reads("/charges")[0];
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
    const details = reads("/charges");
    expect(details).toHaveLength(1);
    expect(details[0]?.pathname).toContain(BILLING_WORK);
    expect(details[0]?.searchParams.get("activity_type")).toBe(BILLING_ACTIVITY_SETUP);
    expect(html).toContain("Activity charge details");
    expect(html.replaceAll(/<!--.*?-->/g, "")).toContain("Setup chat charge details");
  });

  it.each([{ detailStatus: 403 }, { detail: { items: [{ charged_nanos: "broken" }] } }])("detail failure retains period totals and provides retry: %j", async options => {
    backend(options);
    const html = await markup({ activity: BILLING_SELECTION });
    expect(html).toContain("Charges unavailable");
    expect(html).toContain("Retry charges");
    expect(html).toContain("Settled charges");
  });

  it("ignores a disclosure naming a work item outside the visible page", async () => {
    backend();
    await markup({ activity: "foreign-work" });
    expect(reads("/charges")).toHaveLength(0);
  });
});
