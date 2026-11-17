import {Suspense, useEffect, useState} from "react";
import {AppRouterContext} from "next/dist/shared/lib/app-router-context.shared-runtime";
import {PathnameContext, SearchParamsContext} from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import {TooltipProvider, UnavailableState} from "@agentsfleet/design-system";
import {ShellFrame} from "@/components/layout/ShellFrame";
import {UsageLayout} from "@/app/(dashboard)/settings/billing/components/UsageLayout";
import {BillingReportView} from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import {loadBillingReport} from "@/app/(dashboard)/settings/billing/lib/load-report";
import {ModelLabelsProvider} from "@/components/domain/models/ModelLabels";
import {modelLabelsOf} from "@/lib/model-labels";
import {BILLING_CATALOGUE} from "@/tests/helpers/billing-report";
import {OWN_ACCOUNT} from "@/tests/helpers/workspace-fixtures";
import {WORKSPACE} from "@/tests/helpers/draft-fixtures";
import {RouteLoading} from "@/components/layout/RouteLoading";

/** Real report decoder, loader and presentation; browser tests supply HTTP responses and the layout's labels come from the fixture catalogue. */
const LABELS = modelLabelsOf(BILLING_CATALOGUE.models);

export function UsageBrowserFixture() {
  const [query, setQuery] = useState(() => new URLSearchParams(location.search));
  const [pendingLabels] = useState(() => {
    const delay = Number(new URLSearchParams(location.search).get("labelDelay") ?? 0);
    return delay > 0 ? new Promise<typeof LABELS>(resolve => {setTimeout(() => resolve(LABELS), delay);}) : undefined;
  });
  const [loaded, setLoaded] = useState<Awaited<ReturnType<typeof loadBillingReport>> | null>(null);
  useEffect(() => {
    let current = true;
    void loadBillingReport("fixture-token", {...Object.fromEntries(query), activity: undefined, dc: undefined}).then(result => {if (current) setLoaded(result);});
    return () => {current = false;};
  }, [query]);
  const router = {bfcacheId: "usage-review", back() {}, forward() {}, refresh() {setQuery(new URLSearchParams(query));},
    push(url: string) {setQuery(new URL(url, location.origin).searchParams);}, replace() {}, async prefetch() {}};
  const workspaces = [{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}];
  return <AppRouterContext value={router}><PathnameContext value="/settings/billing"><SearchParamsContext value={query}><TooltipProvider>
    <ShellFrame workspaces={workspaces}><ModelLabelsProvider labels={LABELS} pending={pendingLabels}><UsageLayout>
      <Suspense fallback={<RouteLoading />}>
      {loaded === null ? <output>Loading sample report…</output> : loaded.ok
        ? <BillingReportView report={loaded.report} />
        : <UnavailableState title="Billing report unavailable" description="The simulated report request failed." />}
      </Suspense>
    </UsageLayout></ModelLabelsProvider></ShellFrame>
  </TooltipProvider></SearchParamsContext></PathnameContext></AppRouterContext>;
}
