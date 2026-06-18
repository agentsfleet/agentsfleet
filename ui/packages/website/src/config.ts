const fromEnv = import.meta.env.VITE_APP_BASE_URL?.trim();

// agentsfleet hard cutover: APP_BASE_URL + the *.agentsfleet.net hosts are
// flipped ahead of DNS — the product is down until the app/api hosts stand
// up and Clerk JWT aud is configured. config.test.ts pins the new values so
// a regression to the retired brand is a conscious test edit.
//
// Retained deliberately: the only UI consumer (the /agents "open dashboard"
// link) was removed in the fleet-positioning PR, but this stays as the canonical
// env-overridable app host (config.test.ts pins it) for when a dashboard link
// returns. Unlike the deleted TEAM_EMAIL, this value is correct and load-bearing
// config, not a dead, contradictory constant.
export const APP_BASE_URL = fromEnv || (
  import.meta.env.PROD
    ? "https://app.agentsfleet.net"
    : "https://app.dev.agentsfleet.net"
);

// Clerk-hosted Account Portal waitlist page. "Get early access" links here
// (hero, topbar, pricing usage tier) rather than embedding a Clerk form on the
// marketing SPA — the dashboard owner enables Waitlist mode in Clerk and the
// page is themed by the same appearance settings the app already configures.
// PROD is the production Account Portal on the agentsfleet.net custom domain
// (verified Clerk-served; returns 403 until Waitlist sign-up mode is enabled).
// Dev is the Clerk dev instance Account Portal (slug from the dev publishable
// key: winning-wombat-65.accounts.dev). Env-overridable; PROD/dev split mirrors
// APP_BASE_URL.
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

// Bootstrap one-liner — one command that installs agentsfleet AND the skill
// bundle (host-detected) via the agentsfleet.dev installer. Bare-root form (no
// /install.sh path) per the M75 canonical one-liner. Surfaced by the Hero
// copy-row (clipboard payload + visible label).
export const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";
