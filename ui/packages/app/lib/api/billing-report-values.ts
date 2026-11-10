// Shared by browser controls and server-side decoding. Keep transport imports
// in billing_report.ts so filter values cannot pull retry policy into clients.
export const BILLING_PERIOD_MONTH = "month";
export const BILLING_PERIOD_TODAY = "today";
export const BILLING_ACTIVITY_RUNTIME = "runtime";
export const BILLING_ACTIVITY_SETUP = "setup";
// What one charge bills, as `billing.usage_lines.kind` stores it (M245): a model
// call, sandbox run time, or a native decision call.
export const BILLING_CHARGE_KIND = {
  model: "model",
  runtime: "runtime",
  decision: "decision",
} as const;
export type BillingChargeKind = (typeof BILLING_CHARGE_KIND)[keyof typeof BILLING_CHARGE_KIND];
