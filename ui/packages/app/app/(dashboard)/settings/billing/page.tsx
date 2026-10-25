import type { BillingSearch } from "./lib/report-query";
import Link from "next/link";
import { Button, PageHeader, PageLayout, PageTitle, UnavailableState } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { SUPPORT_EMAIL } from "@/lib/contact";
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
  return <PageLayout className="billing-page gap-lg font-system [&_button]:font-system">
    <PageHeader className="min-h-9 flex-row items-center gap-lg border-0 pb-0 sm:gap-lg" actions={
      <Button size="sm" asChild><a href={`mailto:${SUPPORT_EMAIL}`}>Add credits</a></Button>
    }><PageTitle className="font-system text-heading font-medium">Usage &amp; billing</PageTitle></PageHeader>
    {loaded.ok ? <BillingReportView report={loaded.report} modelNames={loaded.modelNames} pageSize={loaded.pageSize}
      selected={loaded.selected} detail={loaded.detail} /> :
      <UnavailableState title="Billing report unavailable" description="Your settled spending could not be loaded. Retry, or reset the filters and try again."
        actions={<><RouteRetryButton label="Retry report" /><Button variant="ghost" size="sm" asChild><Link href="/settings/billing">Reset filters</Link></Button></>} />}
  </PageLayout>;
}
