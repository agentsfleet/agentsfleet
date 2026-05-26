// Single canonical contact email for the Zig server. Mirrors
// `SUPPORT_EMAIL` in ui/packages/website/src/lib/contact.ts,
// ui/packages/app/lib/contact.ts, zombiectl/src/lib/contact.js, and
// ~/Projects/docs/snippets/contact.mdx — cross-tier parity rule says
// the identifier matches across every runtime, so a future address
// rotation lands as a coordinated bump in all five places.
pub const SUPPORT_EMAIL = "usezombie@agentmail.to";
