/*
 * Single source of truth for agentsfleet rates on the marketing site.
 *
 * Server-side authority lives in src/state/tenant_billing.zig (Zig
 * constants NANOS_PER_USD, STARTER_CREDIT_NANOS, EVENT_NANOS,
 * RUN_NANOS_PER_SEC). Identifier names match across Zig + TS + JS
 * (cross-tier parity rule); paired pin tests in rates.test.ts (TS) +
 * tenant_billing_test.zig ("rates pinned") fail on either side until
 * the other catches up.
 *
 * Nanos are held as bigint so the type is exact past
 * Number.MAX_SAFE_INTEGER even though every value used today fits in
 * a JS Number. The whole point of nanos is sub-cent precision; bigint
 * everywhere keeps the type discipline uniform.
 *
 * Display strings ship pre-formatted so the three callers
 * (components/Pricing.tsx, components/FAQ.tsx, pages/Terms.tsx)
 * never re-derive currency math. RATES_DISPLAY keys mirror the
 * Mintlify snippet at ~/Projects/docs/snippets/rates.mdx — bumping a
 * value requires a paired PR there.
 */

export const NANOS_PER_USD = 1_000_000_000n;

export const STARTER_CREDIT_NANOS = 5n * NANOS_PER_USD;
export const EVENT_NANOS = 0n;
// Per-second run rate ($0.0001/sec ≈ $0.36/hr), charged identically under both
// postures while a Fleet is actively running. Replaces the former flat
// per-stage fees — runtime is metered by the second, not per stage.
export const RUN_NANOS_PER_SEC = 100_000n;

// The two FREE_TRIAL_* strings below are marketing copy, not a pricing gate.
// They describe the $5 starter grant — which is still the whole free allowance —
// and stay accurate now that the promotional window is gone. Customer surface
// for live rates: agentsfleet.net/#pricing.
export const RATES_DISPLAY = {
  STARTER_CREDIT: "$5",
  EVENT_RATE: "free",
  // Run rate shown as the per-second billing unit and its hourly equivalent.
  // Usage-based: only billed while a Fleet is actively running, identical
  // under both postures (RUN_NANOS_PER_SEC = 100_000n → $0.0001/sec, ×3600/1e9
  // → $0.36/hr).
  RUN_RATE_PER_SEC: "$0.0001/sec",
  RUN_RATE_PER_HOUR: "$0.36/hr",
  HEADLINE: "Get early access",
  FREE_TRIAL_BANNER:
    "Free during early access — every event receipt and stage execution is on us while we gather traction.",
  FREE_TRIAL_PILL: "Free during early access",
} as const;
