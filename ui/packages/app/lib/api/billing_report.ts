import { z } from "zod";
import { request } from "./client";
import { Instant } from "./decode";
import { nanosSchema } from "./nanos";
import { HTTP_METHOD } from "./retry";
import { BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY, BILLING_PERIOD_WEEK } from "./billing-report-values";

export { BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY, BILLING_PERIOD_WEEK } from "./billing-report-values";

const Count = z.number().int().nonnegative();
// The server counts activities only where a group is a set of activities: the
// period and each fleet. A day or a model row omits the count.
const Totals = z.object({
  charged_nanos: nanosSchema,
  input_tokens: Count, output_tokens: Count, runtime_ms: Count,
  line_count: Count, activity_count: Count.optional(), unpriced_tokens: Count,
});
const CountedTotals = Totals.extend({ activity_count: Count });
const Breakdown = z.object({ key: z.string().nullable(), label: z.string().nullable(), totals: Totals });
const FleetBreakdown = Breakdown.extend({ models: z.array(z.string().min(1)), totals: CountedTotals });
const Daily = z.object({ day: z.iso.date(), totals: Totals });
const Report = z.object({
  period: z.enum([BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY, BILLING_PERIOD_WEEK]), from_at: Instant, through_at: Instant,
  totals: CountedTotals, daily: z.array(Daily),
  spending: z.object({ by_fleet: z.array(FleetBreakdown), by_model: z.array(Breakdown) }),
  filters: z.object({
    fleets: z.array(z.object({ id: z.string().nullable(), name: z.string().nullable() })),
    models: z.array(z.string()),
  }),
});

export type BillingReport = z.infer<typeof Report>;
export type BillingBreakdown = z.infer<typeof Breakdown>;
export type BillingFleetBreakdown = z.infer<typeof FleetBreakdown>;
export type BillingTotals = z.infer<typeof Totals>;
export type BillingCountedTotals = z.infer<typeof CountedTotals>;
export type BillingReportOptions = {
  period?: string; fleet_id?: string; activity_type?: string; through_at?: string;
};

/** Settled totals are supplied by the server across the entire filtered period. */
export async function getBillingReport(token: string, options: BillingReportOptions): Promise<BillingReport> {
  return Report.parse(await request<unknown>(
    `/v1/tenants/me/billing/reports/current?${parameters(options)}`,
    { method: HTTP_METHOD.GET }, token,
  ));
}

function parameters(options: Readonly<Record<string, string | undefined>>): URLSearchParams {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(options)) {
    if (value !== undefined) params.set(key, value);
  }
  return params;
}
