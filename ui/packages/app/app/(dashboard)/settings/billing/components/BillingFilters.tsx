"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import { useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Spinner } from "@agentsfleet/design-system";
import { BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY } from "@/lib/api/billing-report-values";
import { ALL_FLEETS_LABEL, ALL_MODELS_LABEL, SETUP_LABEL } from "../lib/report-format";
import { BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter } from "../lib/report-query";

type Props = { report: BillingReport; modelNames: Record<string, string> };
type Choice = { value: string; label: string };

export function BillingFilters({ report, modelNames }: Props) {
  const params = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const fleets = report.filters.fleets.flatMap(fleet => fleet.id === null ? []
    : [{ value: fleet.id, label: fleet.name ?? fleet.id }]);
  const change = (key: string, value: string) => {
    const next = changedFilter(new URLSearchParams(params.toString()), key, value);
    startTransition(() => router.push(`${pathname}?${next}`, { scroll: false }));
  };
  return <div className="relative flex flex-wrap items-end gap-lg" aria-busy={pending}>
    <Filter label="Period" value={report.period} disabled={pending}
      choices={[{ value: BILLING_PERIOD_MONTH, label: "Month to date" }, { value: BILLING_PERIOD_TODAY, label: "Today" }]}
      onChange={value => change(BILLING_QUERY.period, value)} />
    <Filter label="Fleet" value={params.get(BILLING_QUERY.fleet) ?? BILLING_ALL} disabled={pending}
      choices={[{ value: BILLING_ALL, label: ALL_FLEETS_LABEL }, { value: BILLING_SETUP, label: SETUP_LABEL }, ...fleets]}
      onChange={value => change(BILLING_QUERY.fleet, value)} />
    <Filter label="Model" value={params.get(BILLING_QUERY.model) ?? BILLING_ALL} disabled={pending}
      choices={[{ value: BILLING_ALL, label: ALL_MODELS_LABEL }, ...report.filters.models.map(model => ({ value: model, label: modelNames[model] ?? model }))]}
      onChange={value => change(BILLING_QUERY.model, value)} />
    {pending ? <span className="absolute right-0 top-0"><Spinner size="sm" label="Updating report…" /></span> : null}
  </div>;
}

function Filter({ label, value, choices, onChange, disabled }: {
  label: string; value: string; choices: Choice[]; onChange: (value: string) => void; disabled: boolean;
}) {
  return <div className="flex min-w-0 flex-1 basis-32 flex-col gap-sm">
    <Label className="mb-0 block font-system text-eyebrow font-normal leading-eyebrow">{label}</Label>
    <Select value={value} onValueChange={onChange} disabled={disabled}>
      <SelectTrigger aria-label={label} className="w-full font-system text-label font-normal [&>span:first-child]:min-w-0 [&>span:first-child]:truncate [&>span:first-child]:text-left"><SelectValue /></SelectTrigger>
      <SelectContent className="font-system">{choices.map(choice => <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>)}</SelectContent>
    </Select>
  </div>;
}
