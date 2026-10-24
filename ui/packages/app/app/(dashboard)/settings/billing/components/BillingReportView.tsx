"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCount } from "@/lib/events/run-figures-format";
import { DashboardPanel, MetaGrid, Tabs, TabsContent, TabsList, TabsTrigger, Time } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { BillingFilters } from "./BillingFilters";
import { BillingActivities } from "./BillingActivities";
import { BillingSpending } from "./BillingSpending";

const VIEW = { activity: "activity", spending: "spending" } as const;
type Props = { report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string };

export function BillingReportView(props: Props) {
  const { report, modelNames } = props;
  return <div className="space-y-2xl">
    <BillingFilters report={report} modelNames={modelNames} />
    <DashboardPanel padding="compact">
      <MetaGrid columns={3} items={[
        { label: "Settled spend", value: <span className="text-display-md tabular-nums">{formatBalance(report.totals.charged_nanos)}</span> },
        { label: "Billed activities", value: <span className="text-display-md tabular-nums">{formatCount(report.totals.activity_count)}</span> },
        { label: "Settled charges", value: <span className="text-display-md tabular-nums">{formatCount(report.totals.line_count)}</span> },
      ]} />
      <p className="mt-lg text-body-sm text-muted-foreground">Across the full selected period, through <Time value={report.through_at} format="absolute" />.</p>
      <p className="mt-sm text-body-sm text-muted-foreground">Pending or unavailable provider usage can arrive later and is excluded from these totals. It does not mean free usage.</p>
    </DashboardPanel>
    <Tabs defaultValue={VIEW.activity}>
      <TabsList aria-label="Billing views">
        <TabsTrigger value={VIEW.activity}>Activity</TabsTrigger>
        <TabsTrigger value={VIEW.spending}>Spending</TabsTrigger>
      </TabsList>
      <TabsContent value={VIEW.activity}><BillingActivities {...props} /></TabsContent>
      <TabsContent value={VIEW.spending}><BillingSpending report={report} modelNames={modelNames} /></TabsContent>
    </Tabs>
  </div>;
}
