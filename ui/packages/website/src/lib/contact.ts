// Single canonical contact email for the marketing site. Mirrors
// `SUPPORT_EMAIL` in src/config/contact.zig, ui/packages/app/lib/contact.ts,
// cli/src/lib/contact.js, and ~/Projects/docs/snippets/contact.mdx —
// cross-tier parity rule says the identifier matches across every runtime,
// so a future address rotation lands as a coordinated bump in all five
// places. Surfaces using this constant: Pricing.tsx (design-partner CTA),
// Terms.tsx (§8 contact), Privacy.tsx (§7 contact), CTABlock.tsx if added.
export const SUPPORT_EMAIL = "agentsfleet@agentmail.to";
