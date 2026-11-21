import { useState, type ReactNode } from "react";
import { Time } from "@agentsfleet/design-system";
import type { BillingReport } from "@/lib/api/billing_report";
import { BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY, BILLING_PERIOD_WEEK } from "@/lib/api/billing-report-values";
import { reportingMonths } from "../lib/report-months";
import { BillingFilter } from "./BillingFilter";

type Props = {
  report: BillingReport;
  hasCutoff: boolean;
  disabled: boolean;
  onChange: (period: string, through?: string) => void;
};

export function BillingPeriodFilter({report, hasCutoff, disabled, onChange}: Props) {
  const [calendarNow, setCalendarNow] = useState(() => new Date());
  const months = reportingMonths(calendarNow, report.from_at);
  const monthKey = report.from_at.slice(0, 7);
  const month = months.find(choice => choice.value === monthKey);
  const choices: {value: string; label: ReactNode}[] = months.map(choice => ({
    value: choice.value, label: <Time value={choice.instant} format="month" />,
  }));
  let selected = report.period === BILLING_PERIOD_MONTH ? monthKey : report.period;
  if (report.period === BILLING_PERIOD_MONTH && hasCutoff
      && (!month?.through || Date.parse(month.through) !== Date.parse(report.through_at))) {
    selected = "partial-month";
    choices.unshift({
      value: selected,
      label: <><Time value={report.from_at} format="month" /> · through <Time value={report.through_at} format="date" /></>,
    });
  }
  choices.push({value: BILLING_PERIOD_WEEK, label: "Last 7 days"});
  if (report.period === BILLING_PERIOD_TODAY) choices.push({value: BILLING_PERIOD_TODAY, label: "Today"});

  function changePeriod(value: string) {
    const selectedMonth = months.find(choice => choice.value === value);
    onChange(selectedMonth ? BILLING_PERIOD_MONTH : value, selectedMonth?.through);
  }

  return <BillingFilter label="Period" value={selected} choices={choices} disabled={disabled}
    onChange={changePeriod} onOpenChange={open => {if (open) setCalendarNow(new Date());}} />;
}
