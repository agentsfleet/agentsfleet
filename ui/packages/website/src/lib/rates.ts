/*
 * Single source of truth for hosted-execution rates on the marketing site.
 *
 * Server-side authority lives in src/state/tenant_billing.zig (the Zig
 * constants EVENT_PLATFORM_CENTS, EVENT_BYOK_CENTS, STAGE_CENTS,
 * STARTER_CREDIT_CENTS). When those change, update the values here in
 * lockstep — paired regression tests in rates.test.ts (TS) +
 * tenant_billing_test.zig ("rates pinned") fail on either side until the
 * other catches up.
 *
 * Callers: components/Pricing.tsx, pages/Terms.tsx, components/FAQ.tsx.
 * Display strings are pre-formatted to keep callers from re-deriving the
 * same currency math in three different places. Marketing surfaces show
 * the platform rate; eventByok exists so the paired pin can lock both
 * postures, not because we render it today.
 */

export const RATES_CENTS = {
  /** Per-event receipt charge under platform-managed posture. Mirrors EVENT_PLATFORM_CENTS. */
  eventPlatform: 1,
  /** Per-event receipt charge under BYOK posture. Mirrors EVENT_BYOK_CENTS. */
  eventByok: 0,
  /** Per-stage execution overhead, flat across postures. Mirrors STAGE_CENTS. */
  stage: 10,
  /** One-time credit on tenant signup. Mirrors STARTER_CREDIT_CENTS. */
  starterCredit: 500,
} as const;

export const RATES_DISPLAY = {
  eventPlatform: "$0.01",
  eventByok: "$0",
  stage: "$0.10",
  starterCredit: "$5",
} as const;

/**
 * Worked example shown on the pricing surface. Centralized so the math
 * stays consistent everywhere it's quoted. Uses the platform rate (the
 * higher of the two postures) so the headline drain is the conservative
 * number, not the BYOK best case.
 */
export const WORKED_EXAMPLE = {
  events: 100,
  stagesPerEvent: 3,
  /** 100 × $0.01 + 300 × $0.10 = $31.00 */
  total: "$31.00",
  starterCoversEvents: 16,
} as const;
