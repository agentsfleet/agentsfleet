import { previousReportingMonthEnd } from "@agentsfleet/design-system";

const RECENT_MONTHS = 12;
type Month = {value: string; instant: string; through?: string};

/** Reporting months follow the API calendar; labels never regroup monetary buckets. */
export function reportingMonths(now: Date, from: string): Month[] {
  const current = monthIndex(now.toISOString().slice(0, 7));
  const months = Array.from({length: RECENT_MONTHS}, (_, index) => monthChoice(current - index, current));
  const selected = monthIndex(from.slice(0, 7));
  for (const serial of [selected, selected - 1]) {
    const choice = monthChoice(serial, current);
    if (!months.some(month => month.value === choice.value)) months.push(choice);
  }
  return months;
}

function monthIndex(value: string): number {
  return Number(value.slice(0, 4)) * 12 + Number(value.slice(5, 7)) - 1;
}

function monthValue(serial: number): string {
  return `${String(Math.floor(serial / 12)).padStart(4, "0")}-${String(serial % 12 + 1).padStart(2, "0")}`;
}

function monthChoice(serial: number, current: number): Month {
  const value = monthValue(serial);
  return {value, instant: `${value}-15T12:00:00Z`, ...(serial === current ? {} : {
    through: previousReportingMonthEnd(`${monthValue(serial + 1)}-15T12:00:00Z`),
  })};
}
