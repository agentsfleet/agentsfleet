"use client";

import type { BillingReport } from "@/lib/api/billing_report";
import { useState, useTransition, type ReactNode } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Button, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Spinner, Time } from "@agentsfleet/design-system";
import { BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY, BILLING_PERIOD_WEEK } from "@/lib/api/billing-report-values";
import { ALL_FLEETS_LABEL, SETUP_LABEL } from "../lib/report-format";
import { BILLING_ALL, BILLING_QUERY, BILLING_SETUP, changedFilter, selectedBillingMonth } from "../lib/report-query";
import { reportingMonths } from "../lib/report-months";

type Props = { report: BillingReport };
type Choice = { value: string; label: ReactNode };

export function BillingFilters({ report }: Props) {
  const params = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const [pending, startTransition] = useTransition();
  const fleets = report.filters.fleets.flatMap(fleet => fleet.id === null ? []
    : [{ value: fleet.id, label: fleet.name ?? fleet.id }]);
  const [calendarNow, setCalendarNow] = useState(() => new Date());
  const months = reportingMonths(calendarNow, report.from_at);
  const change = (key: string, value: string, through?: string) => {
    const current = new URLSearchParams(params.toString());
    const next = key === BILLING_QUERY.period && value === BILLING_PERIOD_MONTH
      ? selectedBillingMonth(current, through) : changedFilter(current, key, value);
    startTransition(() => router.push(`${pathname}?${next}`, { scroll: false }));
  };
  const fleet = params.get(BILLING_QUERY.fleet) ?? BILLING_ALL;
  const model = params.get(BILLING_QUERY.model);
  const periodChoices = months.map(month => ({value: month.value, label: <Time value={month.instant} format="month" />}));
  const month = months.find(choice => choice.value === report.from_at.slice(0, 7));
  const captured = report.period === BILLING_PERIOD_MONTH && params.has(BILLING_QUERY.through)
    && (!month?.through || Date.parse(month.through) !== Date.parse(report.through_at));
  if (captured) periodChoices.unshift({value: "partial-month", label: <><Time value={report.from_at} format="month" /> · through <Time value={report.through_at} format="date" /></>});
  const period = captured ? "partial-month" : report.period === BILLING_PERIOD_MONTH ? report.from_at.slice(0, 7) : report.period;
  return <div className="space-y-md"><div className="relative flex flex-wrap items-center gap-xl [&>div:has([role=combobox])]:w-full sm:[&>div:has([role=combobox])]:w-auto" aria-busy={pending}>
    <Filter label="Period" value={period} disabled={pending}
      onOpenChange={open => {if (open) setCalendarNow(new Date());}}
      choices={[...periodChoices, {value: BILLING_PERIOD_WEEK, label: "Last 7 days"}, ...(report.period === BILLING_PERIOD_TODAY ? [{value: BILLING_PERIOD_TODAY, label: "Today"}] : [])]}
      onChange={value => {const month = months.find(choice => choice.value === value); change(BILLING_QUERY.period, month ? BILLING_PERIOD_MONTH : value, month?.through);}} />
    <Filter label="Fleet" value={fleet} disabled={pending}
      choices={[{ value: BILLING_ALL, label: ALL_FLEETS_LABEL }, ...(fleet === BILLING_SETUP ? [{value: BILLING_SETUP, label: SETUP_LABEL}] : []), ...fleets]}
      onChange={value => change(BILLING_QUERY.fleet, value)} />
    {pending ? <span className="absolute right-0 top-0"><Spinner size="sm" label="Updating report…" /></span> : null}
  </div>
    {model ? <div className="flex flex-wrap items-center gap-md text-eyebrow text-muted-foreground" aria-label="Active model filter">
      <span className="min-w-0 break-words [overflow-wrap:anywhere]">Model: {model}</span>
      <Button size="sm" variant="ghost" disabled={pending} onClick={() => change(BILLING_QUERY.model, BILLING_ALL)}>Clear model filter</Button>
    </div> : null}
  </div>;
}

function Filter({ label, value, choices, onChange, disabled, onOpenChange }: {
  label: string; value: string; choices: Choice[]; onChange: (value: string) => void; disabled: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return <div className="flex min-w-0 items-center gap-md">
    <Label className="mb-0 block font-sans text-eyebrow font-normal leading-label text-muted-foreground">{label}</Label>
    <Select value={value} onValueChange={onChange} disabled={disabled} onOpenChange={onOpenChange}>
      <SelectTrigger aria-label={label} className="ml-auto h-10 min-h-10 w-auto min-w-35 max-w-[65%] font-sans text-label font-normal sm:ml-0 sm:max-w-64 [&>span:first-child]:min-w-0 [&>span:first-child]:truncate [&>span:first-child]:text-left"><SelectValue /></SelectTrigger>
      <SelectContent className="font-sans">{choices.map(choice => <SelectItem key={choice.value} value={choice.value}>{choice.label}</SelectItem>)}</SelectContent>
    </Select>
  </div>;
}
