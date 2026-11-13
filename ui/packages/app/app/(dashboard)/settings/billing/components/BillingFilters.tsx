"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import { useTransition, type ReactNode } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Button, Popover, PopoverContent, PopoverTrigger, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Spinner, Time } from "@agentsfleet/design-system";
import { BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY } from "@/lib/api/billing-report-values";
import { ALL_FLEETS_LABEL, ALL_MODELS_LABEL, SETUP_LABEL } from "../lib/report-format";
import { BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter, previousBillingMonth } from "../lib/report-query";

type Props = { report: BillingReport; modelNames: Record<string, string> };
type Choice = { value: string; label: ReactNode };

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
  const previousMonth = () => {
    const next = previousBillingMonth(new URLSearchParams(params.toString()), report.from_at);
    startTransition(() => router.push(`${pathname}?${next}`, { scroll: false }));
  };
  const periodLabel = params.has(BILLING_QUERY.through) ? <Time value={report.from_at} format="month" /> : "Month to date";
  return <div className="relative flex flex-wrap items-center gap-xl [&>div:has([role=combobox])]:w-full sm:[&>div:has([role=combobox])]:w-auto" aria-busy={pending}>
    <Filter label="Period" value={report.period} disabled={pending}
      choices={[{ value: BILLING_PERIOD_MONTH, label: periodLabel }, { value: BILLING_PERIOD_TODAY, label: "Today" }]}
      onChange={value => change(BILLING_QUERY.period, value)} />
    <Filter label="Fleet" value={params.get(BILLING_QUERY.fleet) ?? BILLING_ALL} disabled={pending}
      choices={[{ value: BILLING_ALL, label: ALL_FLEETS_LABEL }, { value: BILLING_SETUP, label: SETUP_LABEL }, ...fleets]}
      onChange={value => change(BILLING_QUERY.fleet, value)} />
    <Popover><PopoverTrigger asChild><Button variant="ghost">More filters</Button></PopoverTrigger>
      <PopoverContent className="space-y-xl">
    <Filter label="Model" value={params.get(BILLING_QUERY.model) ?? BILLING_ALL} disabled={pending}
      choices={[{ value: BILLING_ALL, label: ALL_MODELS_LABEL }, ...report.filters.models.map(model => ({ value: model, label: modelNames[model] ?? model }))]}
      onChange={value => change(BILLING_QUERY.model, value)} />
    <fieldset className="flex flex-wrap gap-sm">
      <legend className="sr-only">Billing month navigation</legend>
      <Button size="sm" variant="outline" disabled={pending} onClick={previousMonth}>Previous month</Button>
      <Button size="sm" variant="ghost" disabled={pending} onClick={() => change(BILLING_QUERY.period, BILLING_PERIOD_MONTH)}>Current month</Button>
    </fieldset>
      </PopoverContent>
    </Popover>
    <p className="ml-auto text-eyebrow text-text-dim">All times UTC</p>
    {pending ? <span className="absolute right-0 top-0"><Spinner size="sm" label="Updating report…" /></span> : null}
  </div>;
}

function Filter({ label, value, choices, onChange, disabled }: {
  label: string; value: string; choices: Choice[]; onChange: (value: string) => void; disabled: boolean;
}) {
  return <div className="flex min-w-0 items-center gap-md">
    <Label className="mb-0 block font-sans text-eyebrow font-normal leading-eyebrow">{label}</Label>
    <Select value={value} onValueChange={onChange} disabled={disabled}>
      <SelectTrigger aria-label={label} className="ml-auto h-10 min-h-10 w-52 max-w-[75%] font-sans text-label font-normal sm:ml-0 sm:w-52 sm:max-w-none [&>span:first-child]:min-w-0 [&>span:first-child]:truncate [&>span:first-child]:text-left"><SelectValue /></SelectTrigger>
      <SelectContent className="font-sans">{choices.map(choice => <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>)}</SelectContent>
    </Select>
  </div>;
}
