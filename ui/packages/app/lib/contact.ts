// Single canonical contact email for the dashboard. Mirrors
// `SUPPORT_EMAIL` in ui/packages/website/src/lib/contact.ts,
// cli/src/lib/contact.ts, and ~/Projects/docs/snippets/contact.mdx. The name
// and the address match on every surface that prints them, so a future
// address rotation lands as one coordinated bump in all four places. The
// daemon prints no support address, so rustd/ holds no copy. Surfaces: BillingBalanceCard exhausted-state mailto, ExhaustionBanner
// support mailto, settings pages with contact CTAs.
export const SUPPORT_EMAIL = "agentsfleet@agentmail.to";
