export const DOCS_URL = "https://docs.agentsfleet.net";
const APP_BASE_URL = import.meta.env.VITE_APP_BASE_URL?.trim().replace(/\/+$/, "") || "https://app.agentsfleet.net";
export const APP_SIGN_IN_URL = `${APP_BASE_URL}/sign-in`;
export const APP_SIGN_UP_URL = `${APP_BASE_URL}/sign-up`;
export const DOCS_QUICKSTART_URL = `${DOCS_URL}/quickstart`;
export const GITHUB_URL = "https://github.com/agentsfleet/agentsfleet";
export const SELF_HOSTED_URL = `${GITHUB_URL}#quick-start`;
export const DISCORD_URL = "https://discord.gg/H9hH2nqQjh";
export const MARKETING_SITE_URL = "https://agentsfleet.net";
// NOTE: the canonical contact address is SUPPORT_EMAIL ("agentsfleet@agentmail.to")
// in src/lib/contact.ts — used by Pricing, Terms, and Privacy. A second
// "team@agentsfleet.net" constant used to live here, unused by any component
// and contradicting the canonical address, so it was removed.
export const MARKETING_LEAD_CAPTURE_URL = import.meta.env.VITE_MARKETING_LEAD_CAPTURE_URL?.trim() || "";
