// Single canonical contact email for the marketing site. Mirrors
// `SUPPORT_EMAIL` in ui/packages/app/lib/contact.ts, cli/src/lib/contact.ts,
// and ~/Projects/docs/snippets/contact.mdx — RULE UFS keeps the identifier
// the same on every surface that carries it, so a future address rotation
// lands as a coordinated bump in all four places. Surfaces using this constant: Pricing.tsx (design-partner CTA),
// Terms.tsx (§8 contact), Privacy.tsx (§7 contact), CTABlock.tsx if added.
export const SUPPORT_EMAIL = "agentsfleet@agentmail.to";
