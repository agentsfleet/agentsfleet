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
// postures while an agent is actively running. Replaces the former flat
// per-stage fees — runtime is metered by the second, not per stage.
export const RUN_NANOS_PER_SEC = 100_000n;

// Promotional free-trial window. While `now_ms < FREE_TRIAL_END_MS` the
// server's `compute_stage_charge` returns FREE_TRIAL_STAGE_NANOS regardless
// of posture / model / tokens. Identifier names match the Zig + JS mirrors.
// Customer surface for live rates + window state: agentsfleet.net/#pricing.
export const FREE_TRIAL_END_MS = 1_785_542_400_000n; // 2026-08-01T00:00:00Z
export const FREE_TRIAL_STAGE_NANOS = 0n;

// Single source of truth for the trial-end *display* string. The pricing
// banner and the landing hero pill both consume this — drifting the date in
// one without the other would mis-message customers. The numeric authority
// is FREE_TRIAL_END_MS above (cross-tier-pinned across Zig + 3 TS surfaces
// by scripts/audit-cross-tier-rates.sh); this is its display-layer mirror.
// IMPORTANT: when FREE_TRIAL_END_MS changes, update this string to match —
// the two are coupled by convention, not by the type system.
const FREE_TRIAL_END_DISPLAY = "July 31, 2026";

export const RATES_DISPLAY = {
  STARTER_CREDIT: "$5",
  EVENT_RATE: "free",
  // Run rate shown as the per-second billing unit and its hourly equivalent.
  // Usage-based: only billed while an agent is actively running, identical
  // under both postures (RUN_NANOS_PER_SEC = 100_000n → $0.0001/sec, ×3600/1e9
  // → $0.36/hr).
  RUN_RATE_PER_SEC: "$0.0001/sec",
  RUN_RATE_PER_HOUR: "$0.36/hr",
  HEADLINE: "Get early access",
  FREE_TRIAL_BANNER: `Free until ${FREE_TRIAL_END_DISPLAY} — every event receipt and stage execution is on us while we gather traction.`,
  FREE_TRIAL_PILL: `Free until ${FREE_TRIAL_END_DISPLAY}`,
} as const;
