/*
 * Single source of truth for hosted-execution rates on the marketing site.
 *
 * Server-side authority lives in src/state/tenant_billing.zig (the Zig
 * constants RECEIVE_PLATFORM_CENTS, STAGE_OVERHEAD_CENTS,
 * STARTER_GRANT_CENTS). When those change, update the values here in
 * lockstep — paired regression tests in rates.test.ts (TS) +
 * tenant_billing_test.zig ("rates pinned") fail on either side until the
 * other catches up.
 *
 * Callers: components/Pricing.tsx, pages/Terms.tsx, components/FAQ.tsx.
 * Display strings are pre-formatted to keep callers from re-deriving the
 * same currency math in three different places.
 */

export const RATES_CENTS = {
  /** Per-event receipt charge. Mirrors RECEIVE_PLATFORM_CENTS. */
  event: 1,
  /** Per-stage execution platform fee. Mirrors STAGE_OVERHEAD_CENTS. */
  stage: 10,
  /** One-time grant on tenant signup. Mirrors STARTER_GRANT_CENTS. */
  starterCredit: 500,
} as const;

export const RATES_DISPLAY = {
  event: "$0.01",
  stage: "$0.10",
  starterCredit: "$5",
} as const;

/**
 * Worked example shown on the pricing surface. Centralized so the math
 * stays consistent everywhere it's quoted.
 */
export const WORKED_EXAMPLE = {
  events: 100,
  stagesPerEvent: 3,
  /** 100 × $0.01 + 300 × $0.10 = $31.00 */
  total: "$31.00",
  starterCoversEvents: 16,
} as const;
