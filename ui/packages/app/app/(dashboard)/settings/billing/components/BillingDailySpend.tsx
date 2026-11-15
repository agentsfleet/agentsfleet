"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import { useSearchParams } from "next/navigation";
import { DashboardPanel, List, ListItem, Time } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { BILLING_ALL, BILLING_QUERY, BILLING_SETUP } from "../lib/report-query";
import { ALL_FLEETS_LABEL, SETUP_LABEL } from "../lib/report-format";

const PERCENT_SCALE = 10_000n;
const PERCENT_DECIMALS = 100;

export function BillingDailySpend({ report }: { report: BillingReport }) {
  const selected = useSearchParams().get(BILLING_QUERY.fleet);
  const fleet = !selected || selected === BILLING_ALL ? ALL_FLEETS_LABEL
    : selected === BILLING_SETUP ? SETUP_LABEL
    : report.filters.fleets.find(item => item.id === selected)?.name ?? selected;
  const largest = report.daily.reduce((maximum, item) => {
    const amount = BigInt(item.totals.charged_nanos);
    return amount > maximum ? amount : maximum;
  }, 0n);
  return <DashboardPanel asChild className="mb-3xl flex min-h-49.5 flex-col bg-background p-xl summary:min-h-51.5 summary:px-2xl summary:py-5">
    <figure aria-label="Daily spend">
      <figcaption className="flex flex-wrap justify-between gap-md text-eyebrow text-muted-foreground">
        <span className="min-w-0 break-words">Daily spend · {fleet}</span>
        <span>Daily high {formatBalance(largest.toString())}</span>
      </figcaption>
      <div className="relative mt-xl">
        <List variant="plain" aria-label="Daily settled charges" className="flex space-y-0 h-25 items-end gap-xs border-b border-border summary:gap-md">
          {report.daily.map(item => {
            const amount = BigInt(item.totals.charged_nanos);
            const height = largest === 0n ? 0 : Number(amount * PERCENT_SCALE / largest) / PERCENT_DECIMALS;
            const label = `${item.day}: ${formatBalance(item.totals.charged_nanos)}`;
            return <ListItem key={item.day} title={label} className="flex h-9/10 min-w-0 flex-1 items-end">
              <span className="sr-only">{label}</span>
              <span aria-hidden="true" className={`w-full rounded-t-[var(--sp-sm)] ${amount === largest ? "bg-primary" : "bg-primary/80"}`}
                style={{ height: `${height}%`, minHeight: amount > 0n ? 2 : 0 }} />
            </ListItem>;
          })}
        </List>
        {largest === 0n ? <p className="absolute inset-0 flex items-center justify-center text-center text-label text-text-dim">No settled charges for this period.</p> : null}
      </div>
      <div className="mt-md flex justify-between gap-md text-eyebrow leading-label text-text-dim">
        <Time value={report.from_at} format="date" />
        <Time value={report.through_at} format="date" />
      </div>
    </figure>
  </DashboardPanel>;
}
