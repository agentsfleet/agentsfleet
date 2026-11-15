"use client";

import type { ReactNode } from "react";
import type { BillingFleetBreakdown, BillingReport } from "@/lib/api/billing_report";
import type { BillingDetail } from "../lib/report-detail";
import { formatCompactCount, formatCount } from "@/lib/events/run-figures-format";
import { Button, Collapsible, CollapsibleContent, CollapsibleTrigger, DashboardPanel, List, ListItem, DataTable, TimeRange, type DataTableColumn } from "@agentsfleet/design-system";
import { CaretDownIcon } from "@phosphor-icons/react/ssr";
import { formatBalance } from "@/lib/money";
import { BillingFilters } from "./BillingFilters";
import { BillingActivities } from "./BillingActivities";
import { BillingSpending } from "./BillingSpending";
import { BillingDailySpend } from "./BillingDailySpend";

type Props = {report: BillingReport; pageSize: number; modelNames: Record<string, string>; detail: BillingDetail; selected: string};

export function BillingReportView(props: Props) {
  const {report, modelNames} = props;
  return <div className="space-y-2xl">
    <BillingFilters report={report} modelNames={modelNames} />
    <DashboardPanel padding="none" className="grid divide-y divide-border summary:grid-cols-[2fr_1fr_1fr] summary:divide-x summary:divide-y-0" aria-label="Period totals">
      <Metric label="Settled charges" value={formatBalance(report.totals.charged_nanos)} note={<><TimeRange from={report.from_at} through={report.through_at} /> · USD</>} primary />
      <Metric label="Activities" value={formatCount(report.totals.activity_count)} note="With settled usage" />
      <Metric label="Tokens" value={formatCompactCount(report.totals.input_tokens + report.totals.output_tokens)} note="Input and output combined" />
    </DashboardPanel>
    <BillingDailySpend report={report} />
    <section className="space-y-xl" aria-label="By fleet">
      <div className="flex flex-wrap items-center justify-between gap-md"><h2 className="text-label font-semibold">By fleet</h2>
        <span className="text-eyebrow text-text-dim"><TimeRange from={report.from_at} through={report.through_at} /></span></div>
      <DataTable columns={fleetColumns(modelNames)} rows={report.spending.by_fleet} rowKey={row => row.key ?? "setup"} caption="Spending by fleet" pagination={false} />
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

function fleetColumns(modelNames: Record<string, string>): DataTableColumn<BillingFleetBreakdown>[] {
  return [
  {key: "fleet", header: "Fleet", primary: true, cell: row => <div><p className="text-label font-semibold">{row.label ?? row.key ?? "Setup chats"}</p><p className="mt-sm text-eyebrow leading-label text-text-dim">{row.key ? "Fleet work" : "Fleet setup"}</p></div>},
  {key: "models", header: "Model", cell: row => row.models.length === 0 ? "No model usage" : <List variant="plain" className="space-y-sm text-label break-words [overflow-wrap:anywhere]">{row.models.map(model => <ListItem key={model}>{modelNames[model] ?? model}</ListItem>)}</List>},
  {key: "activities", header: "Activities", cell: row => formatCount(row.totals.activity_count)},
  {key: "charged", header: "Charged", numeric: true, cell: row => formatBalance(row.totals.charged_nanos)},
  ];
}
