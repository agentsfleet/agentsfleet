"use client";

import { Suspense, type ReactNode } from "react";
import { DashboardPanel, Section, SectionHeader, TimeRange } from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";
import type { BillingReport } from "@/lib/api/billing_report";
import { formatCompactCount, formatCount } from "@/lib/events/run-figures-format";
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
      <DashboardPanel asChild padding="none">
        <dl className="billing-totals" aria-label="Period totals">
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
    <div className="billing-metric">
      <dt>{label}</dt>
      <dd className="billing-metric-value" data-primary={primary}><strong>{value}</strong></dd>
      <dd className="billing-metric-note">{note}</dd>
    </div>
  );
}
