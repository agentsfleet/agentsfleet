const fromEnv = import.meta.env.VITE_APP_BASE_URL?.trim();

// agentsfleet hard cutover: APP_BASE_URL + the *.agentsfleet.net hosts are
// flipped ahead of DNS — the product is down until the app/api hosts stand
// up and Clerk JWT aud is configured. config.test.ts pins the new values so
// a regression to the retired brand is a conscious test edit.
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
export const TEAM_EMAIL = "team@agentsfleet.net";
export const MARKETING_LEAD_CAPTURE_URL = import.meta.env.VITE_MARKETING_LEAD_CAPTURE_URL?.trim() || "";

// Bootstrap one-liner — one command that installs agentsfleet AND the skill
// bundle (host-detected) via the agentsfleet.dev installer. Bare-root form (no
// /install.sh path) per the M75 canonical one-liner. Shared by Hero CTA
// (clipboard payload + visible label) and the Terminal Ledger setup line — single
// source so the two surfaces cannot drift independently.
export const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";

// The platform-ops install skill — step two, run inside the coding agent
// after INSTALL_COMMAND has registered the slash command. INSTALL_SKILL_SLASH
// is the bare command; INSTALL_SKILL_COMMAND is the
// Claude Code invocation the hero terminal demos and copies. Single source.
export const INSTALL_SKILL_SLASH = "/agentsfleet-install-platform-ops";
export const INSTALL_SKILL_COMMAND = `claude ${INSTALL_SKILL_SLASH}`;
