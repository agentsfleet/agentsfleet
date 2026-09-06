// Clerk-hosted Account Portal waitlist page. "Get early access" links here
// (hero and pricing usage tier) rather than embedding a Clerk form on the
// marketing SPA — the dashboard owner enables Waitlist mode in Clerk and the
// page is themed by the same appearance settings the app already configures.
// PROD is the production Account Portal on the agentsfleet.net custom domain
// (verified Clerk-served; returns 403 until Waitlist sign-up mode is enabled).
// Dev is the Clerk dev instance Account Portal (slug from the dev publishable
// key: winning-wombat-65.accounts.dev). Env-overridable per build target.
export const WAITLIST_URL = import.meta.env.VITE_WAITLIST_URL?.trim() || (
  import.meta.env.PROD
    ? "https://accounts.agentsfleet.net/waitlist"
    : "https://winning-wombat-65.accounts.dev/waitlist"
);

export const DOCS_URL = "https://docs.agentsfleet.net";
export const DOCS_QUICKSTART_URL = `${DOCS_URL}/quickstart`;
export const GITHUB_URL = "https://github.com/agentsfleet/agentsfleet";
export const DISCORD_URL = "https://discord.gg/H9hH2nqQjh";
export const MARKETING_SITE_URL = "https://agentsfleet.net";
// NOTE: the canonical contact address is SUPPORT_EMAIL ("agentsfleet@agentmail.to")
// in src/lib/contact.ts — used by Pricing, Terms, and Privacy. A second
// "team@agentsfleet.net" constant used to live here, unused by any component
// and contradicting the canonical address, so it was removed.
export const MARKETING_LEAD_CAPTURE_URL = import.meta.env.VITE_MARKETING_LEAD_CAPTURE_URL?.trim() || "";
