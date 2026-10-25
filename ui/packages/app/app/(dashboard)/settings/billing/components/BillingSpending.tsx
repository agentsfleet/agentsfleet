"use client";

import type { BillingBreakdown, BillingReport } from "@/lib/api/billing_report";
import { EMPTY_STATE_SURFACE, EmptyState, List, ListItem, Section, SectionLabel, UsageBar } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { SETUP_LABEL } from "../lib/report-format";

const BY_FLEET_LABEL = "Charged by fleet or setup chat";
const BY_MODEL_LABEL = "Charged by model";
const BAR_PERCENT_SCALE = 10_000n;
const BAR_PERCENT_DECIMALS = 100;
type Props = { report: BillingReport; modelNames: Record<string, string> };

export function BillingSpending({ report, modelNames }: Props) {
  if (report.spending.by_fleet.length === 0 && report.spending.by_model.length === 0) {
    return <EmptyState title="No settled spending" description="Matching charges will appear here after settlement." surface={EMPTY_STATE_SURFACE.plain} />;
  }
  return <div className="space-y-xl pt-lg font-system">
    <SpendingList title={BY_FLEET_LABEL} rows={report.spending.by_fleet} label={row => row.label ?? row.key ?? SETUP_LABEL} />
    <SpendingList title={BY_MODEL_LABEL} rows={report.spending.by_model} label={row => row.key ? modelNames[row.key] ?? row.key : "Unknown model"} />
  </div>;
}

function SpendingList({ title, rows, label }: { title: string; rows: BillingBreakdown[]; label: (row: BillingBreakdown) => string }) {
  const largest = rows.reduce((maximum, row) => {
    const amount = BigInt(row.totals.charged_nanos);
    return amount > maximum ? amount : maximum;
  }, 0n);
  return <Section className="space-y-lg">
    <SectionLabel variant="navigation" className="mb-0 font-system text-body-sm font-medium text-foreground">{title}</SectionLabel>
    {rows.length === 0 ? <EmptyState title="No settled spending" description="Matching charges will appear here after settlement." surface={EMPTY_STATE_SURFACE.plain} /> :
      <List variant="plain" aria-label={title} className="space-y-lg">{rows.map(row => <ListItem key={row.key ?? SETUP_LABEL}
        className="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto] items-center gap-lg">
        <span className="break-words text-label font-normal">{label(row)}</span>
        <UsageBar className="[&_.usage-bar-track]:h-1.5" pct={largest === 0n ? 0 : Number(BigInt(row.totals.charged_nanos) * BAR_PERCENT_SCALE / largest) / BAR_PERCENT_DECIMALS} />
        <span className="text-label font-normal tabular-nums">{formatBalance(row.totals.charged_nanos)}</span>
      </ListItem>)}</List>}
  </Section>;
}
