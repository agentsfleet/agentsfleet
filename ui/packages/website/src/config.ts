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
