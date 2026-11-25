"use client";

import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Spinner } from "@agentsfleet/design-system";
import type { BillingReport } from "@/lib/api/billing_report";
import { BILLING_PERIOD_MONTH } from "@/lib/api/billing-report-values";
import { ALL_FLEETS_LABEL } from "../lib/report-format";
import { BILLING_ALL, BILLING_QUERY, changedFilter, selectedBillingMonth } from "../lib/report-query";
import { BillingFilter } from "./BillingFilter";
import { BillingPeriodFilter } from "./BillingPeriodFilter";

export function BillingFilters({report}: {report: BillingReport}) {
  const params = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const fleet = params.get(BILLING_QUERY.fleet) || BILLING_ALL;
  const fleets = [{value: BILLING_ALL, label: ALL_FLEETS_LABEL}, ...report.filters.fleets.flatMap(
    item => item.id === null ? [] : [{value: item.id, label: item.name ?? item.id}],
  )];
  if (!fleets.some(choice => choice.value === fleet)) {
    fleets.push({value: fleet, label: fleet});
  }

  function change(key: string, value: string, through?: string) {
    const current = new URLSearchParams(params.toString());
    const next = key === BILLING_QUERY.period && value === BILLING_PERIOD_MONTH
      ? selectedBillingMonth(current, through) : changedFilter(current, key, value);
    startTransition(() => router.push(`${pathname}?${next}`, {scroll: false}));
  }

  return (
    <div className="flex flex-wrap items-center gap-md summary:gap-lg" aria-busy={pending}>
      <BillingPeriodFilter report={report} hasCutoff={params.has(BILLING_QUERY.through)} disabled={pending}
        onChange={(value, through) => change(BILLING_QUERY.period, value, through)} />
      <BillingFilter label="Fleet" value={fleet} choices={fleets} disabled={pending}
        onChange={value => change(BILLING_QUERY.fleet, value)} />
      <span className="text-eyebrow leading-label text-text-dim summary:ml-auto">Periods in UTC · Date ranges in local time</span>
      {pending && <Spinner size="sm" label="Updating report…" />}
    </div>
  );
}
