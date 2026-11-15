import type { BillingSearch } from "../lib/report-query";
import Link from "next/link";
import { Button, UnavailableState } from "@agentsfleet/design-system";
import { RouteRetryButton } from "@/components/domain/shared/navigation/RouteRetryButton";
import { UsageReportView } from "./UsageReportView";
import { loadBillingReport } from "../lib/load-report";

/** The data region: one report request, then the report or a recovery state. */
export async function UsageReport({ token, query }: { token: string; query: BillingSearch }) {
  const loaded = await loadBillingReport(token, query);
  return loaded.ok ? <UsageReportView report={loaded.report} pageSize={loaded.pageSize} selected={loaded.selected} detail={loaded.detail} /> :
    <UnavailableState title="Billing report unavailable" description="Your settled spending could not be loaded. Retry, or reset the filters and try again."
      actions={<><RouteRetryButton label="Retry report" /><Button variant="ghost" size="sm" asChild><Link href="/settings/billing">Reset filters</Link></Button></>} />;
}
