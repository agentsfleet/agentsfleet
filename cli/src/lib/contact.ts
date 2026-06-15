// Single canonical contact email for the CLI. Mirrors `SUPPORT_EMAIL`
// in src/config/contact.zig, ui/packages/website/src/lib/contact.ts,
// ui/packages/app/lib/contact.ts, and ~/Projects/docs/snippets/contact.mdx —
// cross-tier parity rule says the identifier matches across every runtime,
// so a future address rotation lands as a coordinated bump in all five
// places. Surfaces: error help text, CLI doctor command, install skill
// fallback messaging.
export const SUPPORT_EMAIL = "agentsfleet@agentmail.to";
