// Single canonical contact email for the dashboard. Mirrors
// `SUPPORT_EMAIL` in src/config/contact.zig, ui/packages/website/src/lib/contact.ts,
// agentsfleet/src/lib/contact.js, and ~/Projects/docs/snippets/contact.mdx —
// cross-tier parity rule says the identifier matches across every runtime,
// so a future address rotation lands as a coordinated bump in all five
// places. Surfaces: BillingBalanceCard exhausted-state mailto, ExhaustionBanner
// support mailto, settings pages with contact CTAs.
export const SUPPORT_EMAIL = "usezombie@agentmail.to";
