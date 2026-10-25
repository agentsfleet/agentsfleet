import { PROVIDER_MODE } from "@/lib/types";
import { z } from "zod";
import { request } from "./client";
import { Instant } from "./decode";
import { nanosSchema } from "./nanos";
import { HTTP_METHOD } from "./retry";
import { BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY } from "./billing-report-values";

export { BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY } from "./billing-report-values";

const Count = z.number().int().nonnegative();
const Totals = z.object({
  charged_nanos: nanosSchema,
  input_tokens: Count, output_tokens: Count, runtime_ms: Count,
  line_count: Count, activity_count: Count, unpriced_tokens: Count,
});
const Activity = z.object({
  id: z.string(), activity_type: z.enum([BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP]),
  workspace_id: z.string(), fleet_id: z.string().nullable(), fleet_name: z.string().nullable(),
  recorded_at: Instant, totals: Totals, models: z.array(z.string()),
  postures: z.array(z.enum(PROVIDER_MODE)),
});
const Charge = z.object({
  id: z.string(), sequence: Count, model: z.string(), posture: z.enum(PROVIDER_MODE),
  charged_nanos: nanosSchema, input_tokens: Count, output_tokens: Count,
  runtime_ms: Count, unpriced_tokens: Count, requested_at: Instant, settled_at: Instant,
});
const Breakdown = z.object({ key: z.string().nullable(), label: z.string().nullable(), totals: Totals });
const ActivityPage = z.object({
  items: z.array(Activity), total: Count.nullable(), next_cursor: z.string().nullable(),
});
const ChargePage = z.object({
  items: z.array(Charge), total: Count.nullable(), next_cursor: z.string().nullable(),
});
const Report = z.object({
  period: z.enum([BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY]), from_at: Instant, through_at: Instant,
  totals: Totals,
  spending: z.object({ by_fleet: z.array(Breakdown), by_model: z.array(Breakdown) }),
  filters: z.object({
    fleets: z.array(z.object({ id: z.string().nullable(), name: z.string().nullable() })),
    models: z.array(z.string()),
  }),
  activities: ActivityPage,
});

export type BillingReport = z.infer<typeof Report>;
export type BillingActivity = z.infer<typeof Activity>;
export type BillingCharge = z.infer<typeof Charge>;
export type BillingChargePage = z.infer<typeof ChargePage>;
export type BillingBreakdown = z.infer<typeof Breakdown>;
export type BillingTotals = z.infer<typeof Totals>;
export type BillingReportOptions = {
  period?: string; fleet_id?: string; model?: string; activity_type?: string;
  through_at?: string; limit: number; starting_after?: string;
};

/** Settled totals are supplied by the server across the entire filtered period. */
export async function getBillingReport(token: string, options: BillingReportOptions): Promise<BillingReport> {
  return Report.parse(await request<unknown>(
    `/v1/tenants/me/billing/reports/current?${parameters(options)}`,
    { method: HTTP_METHOD.GET }, token,
  ));
}

/** The daemon independently authorizes the account and scopes this opaque work identifier. */
export async function listBillingActivityCharges(
  token: string, id: string, options: BillingReportOptions,
): Promise<BillingChargePage> {
  return ChargePage.parse(await request<unknown>(
    `/v1/tenants/me/billing/activities/${encodeURIComponent(id)}/charges?${parameters(options)}`,
    { method: HTTP_METHOD.GET }, token,
  ));
}

function parameters(options: Readonly<Record<string, string | number | undefined>>): URLSearchParams {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(options)) {
    if (value !== undefined) params.set(key, String(value));
  }
  return params;
}
