import type { BillingChargePage } from "@/lib/api/billing_report";

export const BILLING_DETAIL_STATE = { ready: "ready", unavailable: "unavailable", closed: "closed" } as const;

export type BillingDetail =
  | { status: typeof BILLING_DETAIL_STATE.ready; charges: BillingChargePage; page: number }
  | { status: typeof BILLING_DETAIL_STATE.unavailable }
  | { status: typeof BILLING_DETAIL_STATE.closed };
