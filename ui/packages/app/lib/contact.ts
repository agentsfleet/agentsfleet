// Single canonical contact email for the dashboard. Mirrors
// `SUPPORT_EMAIL` in ui/packages/website/src/lib/contact.ts and
// ~/Projects/docs/snippets/contact.mdx. The name and the address match on every
// surface that prints them, so a future address rotation lands as one
// coordinated bump in all three places. The CLI and the daemon print no support
// address, so cli/ and rustd/ hold no copy. Surfaces: BillingBalanceCard's
// exhausted-state mailto and buy-credits link, ExhaustionBanner's support mailto.
export const SUPPORT_EMAIL = "agentsfleet@agentmail.to";
