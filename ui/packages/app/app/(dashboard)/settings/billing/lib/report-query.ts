import type { BillingReportOptions } from "@/lib/api/billing_report";
import { BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH } from "@/lib/api/billing-report-values";

export const BILLING_QUERY = { period: "period", fleet: "fleet", through: "through_at" } as const;
export const BILLING_ALL = "all";
export const BILLING_SETUP = BILLING_ACTIVITY_SETUP;
export type BillingSearch = Record<string, string | string[] | undefined>;

/** A malformed explicit filter reaches the daemon's validation instead of widening the read. */
export function reportOptions(query: BillingSearch): BillingReportOptions {
  const fleet = first(query[BILLING_QUERY.fleet]);
  return {
    period: first(query[BILLING_QUERY.period]) ?? BILLING_PERIOD_MONTH,
    fleet_id: fleet && fleet !== BILLING_ALL && fleet !== BILLING_SETUP ? fleet : undefined,
    activity_type: fleet === BILLING_SETUP ? BILLING_ACTIVITY_SETUP : undefined,
    through_at: first(query[BILLING_QUERY.through]),
  };
}

/** Filtering retains the selected reporting cutoff; changing period returns to now. */
export function changedFilter(params: URLSearchParams, key: string, value: string): URLSearchParams {
  const next = new URLSearchParams(params);
  if (key === BILLING_QUERY.period) next.delete(BILLING_QUERY.through);
  if (value === BILLING_ALL) next.delete(key);
  else next.set(key, value);
  return next;
}

/** Selecting a named month keeps the other report filters. */
export function selectedBillingMonth(params: URLSearchParams, through?: string): URLSearchParams {
  const next = changedFilter(params, BILLING_QUERY.period, BILLING_PERIOD_MONTH);
  if (through) next.set(BILLING_QUERY.through, through);
  return next;
}

export function first(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}
