"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCompactCount, formatCount } from "@/lib/events/run-figures-format";
import { MetaGrid, Tabs, TabsContent, TabsList, TabsTrigger, Time, formatTimeRangeUtc } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { BillingFilters } from "./BillingFilters";
import { BillingActivities } from "./BillingActivities";
import { BillingSpending } from "./BillingSpending";

const VIEW = { activity: "activity", spending: "spending" } as const;
type Props = { report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string };

export function BillingReportView(props: Props) {
  const { report, modelNames } = props;
  return <div className="space-y-md font-system text-label font-normal">
    <BillingFilters report={report} modelNames={modelNames} />
    <p className="flex flex-wrap justify-between gap-sm text-eyebrow leading-eyebrow text-muted-foreground">
      <span><Time value={report.through_at} label={formatTimeRangeUtc(report.from_at, report.through_at)} /> · Coordinated Universal Time (UTC)</span>
      <span>Settled charges</span>
    </p>
    <MetaGrid columns={3} aria-label="Period totals" className="grid-cols-[1.2fr_1fr_1fr] gap-xl border-y border-border py-2.5 sm:grid-cols-[1.2fr_1fr_1fr] [&>div]:min-w-0 [&_dt]:font-system [&_dt]:normal-case [&_dt]:text-eyebrow [&_dt]:tracking-normal" items={[
      { label: "Charged this period", value: <span className="text-heading font-medium tabular-nums">{formatBalance(report.totals.charged_nanos)}</span> },
      { label: "Billed activities", value: <span className="text-heading font-medium tabular-nums">{formatCount(report.totals.activity_count)}</span> },
      { label: "Input / output tokens", value: <span className="text-heading font-medium tabular-nums">{formatCompactCount(report.totals.input_tokens)}<span className="text-body-sm text-muted-foreground"> / {formatCompactCount(report.totals.output_tokens)}</span></span> },
    ]} />
    <Tabs defaultValue={VIEW.activity}>
      <TabsList aria-label="Billing views">
        <TabsTrigger value={VIEW.activity}>Activity</TabsTrigger>
        <TabsTrigger value={VIEW.spending}>Spending breakdown</TabsTrigger>
      </TabsList>
      <TabsContent value={VIEW.activity} className="mt-0"><BillingActivities {...props} /></TabsContent>
      <TabsContent value={VIEW.spending} className="mt-0"><BillingSpending report={report} modelNames={modelNames} /></TabsContent>
    </Tabs>
    <p className="text-label text-muted-foreground">Totals exclude pending or unavailable provider usage, which can arrive later. Billing history includes deleted fleets.</p>
  </div>;
}
