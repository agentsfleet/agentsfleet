"use client";

import type { ReactNode } from "react";
import type { BillingBreakdown, BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCompactCount, formatCount } from "@/lib/events/run-figures-format";
import { Button, Collapsible, CollapsibleContent, CollapsibleTrigger, DashboardPanel, DataTable, Time, formatTimeRangeUtc, type DataTableColumn } from "@agentsfleet/design-system";
import { CaretDownIcon } from "@phosphor-icons/react/ssr";
import { formatBalance } from "@/lib/money";
import { BillingFilters } from "./BillingFilters";
import { BillingActivities } from "./BillingActivities";
import { BillingSpending } from "./BillingSpending";

type Props = {report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string};

export function BillingReportView(props: Props) {
  const {report, modelNames} = props;
  return <div className="space-y-2xl">
    <BillingFilters report={report} modelNames={modelNames} />
    <DashboardPanel padding="none" className="grid divide-y divide-border summary:grid-cols-[2fr_1fr_1fr] summary:divide-x summary:divide-y-0" aria-label="Period totals">
      <Metric label="Settled charges" value={formatBalance(report.totals.charged_nanos)} note={<Time value={report.through_at} label={`${formatTimeRangeUtc(report.from_at, report.through_at)} · USD`} />} primary />
      <Metric label="Activities" value={formatCount(report.totals.activity_count)} note="With settled usage" />
      <Metric label="Tokens" value={formatCompactCount(report.totals.input_tokens + report.totals.output_tokens)} note="Input and output combined" />
    </DashboardPanel>
    <DashboardPanel asChild className="mb-3xl flex min-h-51.5 flex-col bg-background px-2xl py-5">
      <figure aria-label="Daily spend"><figcaption className="text-eyebrow text-muted-foreground">Daily spend</figcaption>
        <div className="flex flex-1 items-center justify-center py-3xl text-label text-text-dim">Daily spending is not available yet.</div>
        <p className="text-eyebrow text-text-dim">Period totals and the fleet breakdown below include all settled charges.</p>
      </figure>
    </DashboardPanel>
    <section className="space-y-xl" aria-label="By fleet">
      <div className="flex flex-wrap items-center justify-between gap-md"><h2 className="text-label font-semibold">By fleet</h2>
        <span className="text-eyebrow text-text-dim"><Time value={report.through_at} label={formatTimeRangeUtc(report.from_at, report.through_at)} /></span></div>
      <DataTable columns={fleetColumns} rows={report.spending.by_fleet} rowKey={row => row.key ?? "setup"} caption="Spending by fleet" pagination={false} />
    </section>
    <Collapsible defaultOpen={Boolean(props.selected)}>
      <CollapsibleTrigger asChild><Button variant="ghost" className="px-0">Charge details<CaretDownIcon size={16} /></Button></CollapsibleTrigger>
      <CollapsibleContent className="space-y-xl pt-lg"><BillingActivities {...props} /><BillingSpending report={report} modelNames={modelNames} /></CollapsibleContent>
    </Collapsible>
    <p className="text-eyebrow leading-eyebrow text-text-dim">Totals exclude pending or unavailable provider usage, which can arrive later. Billing history includes deleted fleets.</p>
  </div>;
}

function Metric({label, value, note, primary = false}: {label: string; value: string; note: ReactNode; primary?: boolean}) {
  return <div className="grid min-w-0 grid-cols-[1fr_auto] items-center gap-x-xl p-xl leading-label summary:block summary-medium:p-2xl"><p className="summary:mb-md text-eyebrow leading-label font-semibold text-muted-foreground">{label}</p>
    <strong className={`col-start-2 row-span-2 row-start-1 break-words text-usage-total font-[750] leading-label tracking-usage-total summary:block summary:leading-usage-total ${primary ? "text-success" : "text-foreground"}`}>{value}</strong>
    <p className="col-start-1 summary:mt-md text-eyebrow leading-label text-text-dim">{note}</p></div>;
}

const fleetColumns: DataTableColumn<BillingBreakdown>[] = [
  {key: "fleet", header: "Fleet", primary: true, cell: row => <div><p className="text-label font-semibold">{row.label ?? row.key ?? "Setup chats"}</p><p className="mt-sm text-eyebrow leading-label text-text-dim">{row.key ? "Fleet work" : "Fleet setup"}</p></div>},
  {key: "activities", header: "Activities", cell: row => formatCount(row.totals.activity_count)},
  {key: "charged", header: "Charged", numeric: true, cell: row => formatBalance(row.totals.charged_nanos)},
];
