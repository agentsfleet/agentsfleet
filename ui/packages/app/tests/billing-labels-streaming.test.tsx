import {TooltipProvider} from "@agentsfleet/design-system";
import {renderToReadableStream} from "react-dom/server";
import {afterEach, expect, it, vi} from "vitest";
import BillingLayout from "@/app/(dashboard)/settings/billing/layout";
import {BillingReportView} from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import {UsageLayout} from "@/app/(dashboard)/settings/billing/components/UsageLayout";
import type {ModelLabels} from "@/lib/model-labels";
import {BILLING_MODEL, BILLING_MODEL_NAME, billingReport} from "./helpers/billing-report";

const {modelLabelsCached} = vi.hoisted(() => ({modelLabelsCached: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({requireCredential: async () => "fixture-token"}));
vi.mock("@/lib/model-labels-server", () => ({modelLabelsCached}));
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({push() {}}), usePathname: () => "/settings/billing"}));
afterEach(() => vi.clearAllMocks());

async function renderUsage() {
  const content = <UsageLayout><BillingReportView report={billingReport()} /></UsageLayout>;
  return renderToReadableStream(<TooltipProvider>{await BillingLayout({children: content})}</TooltipProvider>);
}

it("streams Usage totals and daily spending while the fleet table waits for model names", async () => {
  const labels = Promise.withResolvers<ModelLabels>();
  modelLabelsCached.mockReturnValue(labels.promise);
  const stream = await renderUsage();
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let prefix = "";
  while (!prefix.includes("Daily spend") || !prefix.includes("Loading page")) {
    const chunk = await reader.read();
    if (chunk.done) break;
    prefix += decoder.decode(chunk.value);
  }
  expect(prefix).toContain("Usage");
  expect(prefix).toContain("Settled charges");
  expect(prefix).toContain("Daily spend");
  expect(prefix).toContain("Loading page");
  expect(prefix).not.toContain(BILLING_MODEL);
  labels.resolve({[BILLING_MODEL]: BILLING_MODEL_NAME});
  let table = "";
  for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) table += decoder.decode(chunk.value);
  expect(table).toContain(BILLING_MODEL_NAME);
  expect(modelLabelsCached).toHaveBeenCalledExactlyOnceWith("fixture-token");
});

it("shows recorded model identifiers and settled usage when the catalogue is unavailable", async () => {
  modelLabelsCached.mockRejectedValue(new Error("Catalogue unavailable"));
  const stream = await renderUsage();
  await stream.allReady;
  const html = await new Response(stream).text();
  expect(html).toContain(BILLING_MODEL);
  expect(html).not.toContain(BILLING_MODEL_NAME);
  expect(html).toContain("$0.0001");
});
