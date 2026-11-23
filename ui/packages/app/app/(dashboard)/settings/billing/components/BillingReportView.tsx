"use client";

import { Suspense, type ReactNode } from "react";
import { DashboardPanel, Section, SectionHeader, TimeRange, cn } from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";
import type { BillingReport } from "@/lib/api/billing_report";
import { formatCompactCount, formatCount } from "@/lib/turns/run-figures-format";
import { formatBalance } from "@/lib/money";
import { BillingDailySpend } from "./BillingDailySpend";
import { BillingFilters } from "./BillingFilters";
import { BillingFleetTable } from "./BillingFleetTable";

/** The fleet table can wait for model labels while the report remains visible. */
export function BillingReportView({report}: {report: BillingReport}) {
  const range = <TimeRange from={report.from_at} through={report.through_at} />;
  return (
    <div className="space-y-2xl">
      <BillingFilters report={report} />
      <DashboardPanel asChild padding="none" className="overflow-hidden bg-border">
        <dl className="grid grid-cols-2 gap-px summary:grid-cols-[2fr_1fr_1fr]" aria-label="Period totals">
          <Metric label="Settled charges" value={formatBalance(report.totals.charged_nanos)} note={<>{range} · USD</>} primary />
          <Metric label="Activities" value={formatCount(report.totals.activity_count)} note="With settled usage" />
          <Metric label="Tokens" value={formatCompactCount(report.totals.input_tokens + report.totals.output_tokens)} note="Input and output combined" />
        </dl>
      </DashboardPanel>
      <BillingDailySpend report={report} />
      <Section aria-label="By fleet">
        <SectionHeader actions={<span className="block text-eyebrow text-text-dim">{range}</span>}>By fleet</SectionHeader>
        <Suspense fallback={<RouteLoading />}><BillingFleetTable rows={report.spending.by_fleet} /></Suspense>
      </Section>
      <p className="text-eyebrow leading-eyebrow text-text-dim">
        Totals exclude pending or unavailable provider usage, which can arrive later. Billing history includes deleted fleets.
      </p>
    </div>
  );
}

function Metric({label, value, note, primary}: {label: string; value: string; note: ReactNode; primary?: boolean}) {
  return (
    <div className={cn("min-w-0 bg-card p-xl leading-label summary-medium:p-2xl",
      primary ? "max-summary:col-span-full max-summary:grid max-summary:grid-cols-[1fr_auto] max-summary:items-start max-summary:gap-x-xl" : "max-summary:space-y-xs")}>
      <dt className="text-eyebrow font-semibold leading-label text-muted-foreground summary:mb-md">{label}</dt>
      <dd className={cn("col-start-2 row-span-2 row-start-1 wrap-anywhere", primary ? "text-success" : "text-foreground")}>
        <strong className="block text-usage-total leading-label font-[750] tracking-usage-total summary:inline">{value}</strong>
      </dd>
      <dd className="col-start-1 text-eyebrow leading-label text-text-dim summary:mt-md">{note}</dd>
    </div>
  );
}
