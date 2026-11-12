import type { BillingSearch } from "./lib/report-query";
import Link from "next/link";
import { Button, UnavailableState } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { UsageLayout } from "./components/UsageLayout";
import { RouteRetryButton } from "@/components/domain/RouteRetryButton";
import { BillingReportView } from "./components/BillingReportView";
import { loadBillingReport } from "./lib/load-report";

export const dynamic = "force-dynamic";

export default async function BillingSettingsPage({
  searchParams,
}: {
  searchParams?: Promise<BillingSearch>;
} = {}) {
  const query = searchParams ? await searchParams : {};
  const token = await requireCredential();
  const loaded = await loadBillingReport(token, query);
  return <UsageLayout>
    {loaded.ok ? <BillingReportView report={loaded.report} modelNames={loaded.modelNames} pageSize={loaded.pageSize}
      selected={loaded.selected} detail={loaded.detail} /> :
      <UnavailableState title="Billing report unavailable" description="Your settled spending could not be loaded. Retry, or reset the filters and try again."
        actions={<><RouteRetryButton label="Retry report" /><Button variant="ghost" size="sm" asChild><Link href="/settings/billing">Reset filters</Link></Button></>} />}
  </UsageLayout>;
}
