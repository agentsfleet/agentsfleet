"use client";

import { Suspense, type ReactNode } from "react";
import type { BillingReport } from "@/lib/api/billing_report";
import { formatCompactCount, formatCount } from "@/lib/events/run-figures-format";
import { DashboardPanel, TimeRange } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { BillingFilters } from "./BillingFilters";
import { BillingDailySpend } from "./BillingDailySpend";
import { BillingFleetTable } from "./BillingFleetTable";
import { RouteLoading } from "@/components/layout/RouteLoading";

type Props = {report: BillingReport};

/** Renders from the report alone; only the fleet table waits for model labels, inside its own boundary. */
export function BillingReportView({report}: Props) {
  return <div className="space-y-2xl">
    <BillingFilters report={report} />
    <DashboardPanel padding="none" className="grid divide-y divide-border summary:grid-cols-[2fr_1fr_1fr] summary:divide-x summary:divide-y-0" aria-label="Period totals">
      <Metric label="Settled charges" value={formatBalance(report.totals.charged_nanos)} note={<><TimeRange from={report.from_at} through={report.through_at} /> · USD</>} primary />
      <Metric label="Activities" value={formatCount(report.totals.activity_count)} note="With settled usage" />
      <Metric label="Tokens" value={formatCompactCount(report.totals.input_tokens + report.totals.output_tokens)} note="Input and output combined" />
    </DashboardPanel>
    <BillingDailySpend report={report} />
    <section className="space-y-xl" aria-label="By fleet">
      <div className="flex flex-wrap items-center justify-between gap-md"><h2 className="text-label font-semibold">By fleet</h2>
        <span className="text-eyebrow text-text-dim"><TimeRange from={report.from_at} through={report.through_at} /></span></div>
      <Suspense fallback={<RouteLoading />}><BillingFleetTable rows={report.spending.by_fleet} /></Suspense>
    </section>
    <p className="text-eyebrow leading-eyebrow text-text-dim">Totals exclude pending or unavailable provider usage, which can arrive later. Billing history includes deleted fleets.</p>
  </div>;
}

function Metric({label, value, note, primary = false}: {label: string; value: string; note: ReactNode; primary?: boolean}) {
  return <div className="grid min-w-0 grid-cols-[1fr_auto] items-center gap-x-xl p-xl leading-label summary:block summary-medium:p-2xl"><p className="summary:mb-md text-eyebrow leading-label font-semibold text-muted-foreground">{label}</p>
    <strong className={`col-start-2 row-span-2 row-start-1 break-words text-usage-total font-[750] leading-label tracking-usage-total ${primary ? "text-success" : "text-foreground"}`}>{value}</strong>
    <p className="col-start-1 summary:mt-md text-eyebrow leading-label text-text-dim">{note}</p></div>;
}
