import React, {Suspense, type ReactNode} from "react";
import {expect, it, vi} from "vitest";
import {renderToReadableStream} from "react-dom/server";
import {UsageLayout} from "@/app/(dashboard)/settings/billing/components/UsageLayout";
import {RouteLoading} from "@/components/layout/RouteLoading";
import {UsageReportView} from "@/app/(dashboard)/settings/billing/components/UsageReportView";
import {BILLING_MODEL, billingReport} from "./helpers/billing-report";
const {modelLabelsCached} = vi.hoisted(() => ({modelLabelsCached: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential: async () => "fixture-token"}));
vi.mock("@/lib/workspace", () => ({listTenantWorkspacesCached: async () => ({items: [], total: 0})}));
vi.mock("@/lib/api/tenant_billing", () => ({getTenantBillingCached: async () => null}));
vi.mock("@/lib/invites", () => ({listWaitingInvitesCached: async () => []}));
vi.mock("@/lib/model-labels-server", () => ({modelLabelsCached}));
vi.mock("@/components/layout/ShellFrame", () => ({ShellFrame: ({children}: {children: ReactNode}) => children}));
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({push() {}}), usePathname: () => "/settings/billing"}));
import DashboardLayout from "@/app/(dashboard)/layout";
it("streams the Usage frame before slow shared labels without painting an incorrect model name", async () => {

    let release = (_labels: Record<string, string>) => {};
    modelLabelsCached.mockReturnValue(new Promise<Record<string, string>>(resolve => {release = resolve;}));
    let ready = false;
    const pending = DashboardLayout({children: <UsageLayout><Suspense fallback={<RouteLoading />}><UsageReportView report={billingReport()} pageSize={25} selected="" detail={{status: "closed"}} /></Suspense></UsageLayout>})
      .then(tree => {ready = true; return tree;});
    await new Promise(resolve => setTimeout(resolve, 0));
    const blockedByLabels = !ready;
    if (blockedByLabels) release({"accounts/fireworks/models/minimax-m3p1-flash": "Named model"});
    const tree = await pending;
    expect(blockedByLabels).toBe(false);
    const stream = await renderToReadableStream(tree);
    const reader = stream.getReader(), decoder = new TextDecoder();
    const prefixRead = (async () => {
      let prefix = "";
      while(!prefix.includes("Daily spend") || !prefix.includes("Loading page")) {
        const chunk = await reader.read();
        if(chunk.done) break;
        prefix += decoder.decode(chunk.value);
      }
      return prefix;
    })();
    let timedOut = false;
    const deadline = setTimeout(() => {timedOut = true; release({[BILLING_MODEL]: "Named model"});}, 1_000);
    const first = await prefixRead;
    clearTimeout(deadline);
    expect(timedOut).toBe(false);
    expect(first).toContain("Usage");
    expect(first).toContain("Settled charges");
    expect(first).toContain("Daily spend");
    expect(first).toContain("Loading page");
    expect(first).not.toContain(BILLING_MODEL);
    release({"accounts/fireworks/models/minimax-m3p1-flash": "Named model"});
    let rest = "";
    for(let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) rest += decoder.decode(chunk.value);
    expect(rest).toContain("Named model");
    expect(modelLabelsCached).toHaveBeenCalledExactlyOnceWith("fixture-token");
  });
