"use client";

import type { BillingBreakdown, BillingReport } from "@/lib/api/billing_report";
import { formatCount } from "@/lib/events/run-figures-format";
import { DataTable, EmptyState, Section, SectionHeader, type DataTableColumn } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { SETUP_LABEL } from "../lib/report-format";

const BY_FLEET_LABEL = "By fleet";
type Props = { report: BillingReport; modelNames: Record<string, string> };

export function BillingSpending({ report, modelNames }: Props) {
  return <div className="grid gap-2xl xl:grid-cols-2">
    <SpendingTable title={BY_FLEET_LABEL} rows={report.spending.by_fleet} label={row => row.label ?? row.key ?? SETUP_LABEL} />
    <SpendingTable title="By model" rows={report.spending.by_model} label={row => row.key ? modelNames[row.key] ?? row.key : "Unknown model"} />
  </div>;
}

function SpendingTable({ title, rows, label }: { title: string; rows: BillingBreakdown[]; label: (row: BillingBreakdown) => string }) {
  const columns: DataTableColumn<BillingBreakdown>[] = [
    { key: "name", header: title === BY_FLEET_LABEL ? "Fleet" : "Model", primary: true, cell: label },
    { key: "activity", header: "Activities", numeric: true, hideOnMobile: true, cell: row => formatCount(row.totals.activity_count) },
    { key: "amount", header: "Settled spend", numeric: true, cell: row => <span className="font-mono tabular-nums">{formatBalance(row.totals.charged_nanos)}</span> },
  ];
  return <Section className="space-y-lg">
    <SectionHeader as="h2">{title}</SectionHeader>
    <DataTable columns={columns} rows={rows} rowKey={row => row.key ?? SETUP_LABEL} caption={title}
      viewportClassName="max-h-none" empty={<EmptyState title="No settled spending" description="Matching charges will appear here after settlement." />} />
  </Section>;
}
