import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToReadableStream } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ModelLabelsProvider } from "@/components/domain/models/ModelLabels";
import { mockAuthOnce } from "./helpers/dashboard-mocks";
import { BILLING_AT, BILLING_MODEL, BILLING_MODEL_NAME, billingReport } from "./helpers/billing-report";
import type { BillingSearch } from "@/app/(dashboard)/settings/billing/lib/report-query";

vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
import BillingSettingsPage from "@/app/(dashboard)/settings/billing/page";
import { UsageReport } from "@/app/(dashboard)/settings/billing/components/UsageReport";

type Backend = { reportStatus?: number; report?: unknown; reportGate?: Promise<void>; reportFailure?: Error };
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
    expect(requests[0]?.pathname).toBe("/backend/v1/tenants/me/billing/reports/current");
    expect(html).not.toContain("Charge details");
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
    const html = await markup();
    expect(html).toContain("Billing report unavailable");
    expect(html).toContain("Retry report");
    expect(html).toContain("Reset filters");
    expect(html).not.toContain("No settled charges for this period.");
    expect(requests).toHaveLength(1);
  });

  it("threads exact filters and cutoff to the report, and drops removed model and paging inputs", async () => {
    backend();
    await markup({ period: "today", model: BILLING_MODEL, fleet: "setup", through_at: BILLING_AT, c: ["previous", "current"], cps: "50", ps: "50", activity: "old", dc: "old" });
    expect(requests).toHaveLength(1);
    expect(Object.fromEntries(reads("/reports/current")[0]?.searchParams ?? [])).toEqual({ period: "today", activity_type: "setup", through_at: BILLING_AT });
  });
});
