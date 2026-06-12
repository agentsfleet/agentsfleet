const fromEnv = import.meta.env.VITE_APP_BASE_URL?.trim();

// agentsfleet rebrand: flips to app.agentsfleet.net / docs.agentsfleet.net
// are PARKED until those hosts answer (DNS + hosting + Clerk origin are
// operator console steps; see the active rebrand spec). Flipping early
// would dead-end every link; config.test.ts pins the parked values so the
// flip is a conscious test edit, not a drive-by.
export const APP_BASE_URL = fromEnv || (
  import.meta.env.PROD
    ? "https://app.usezombie.com"
    : "https://app.dev.usezombie.com"
);

export const DOCS_URL = "https://docs.usezombie.com";
export const DOCS_QUICKSTART_URL = `${DOCS_URL}/quickstart`;
export const GITHUB_URL = "https://github.com/usezombie/usezombie";
export const DISCORD_URL = "https://discord.gg/H9hH2nqQjh";
export const TEAM_EMAIL = "team@usezombie.com";
export const MARKETING_LEAD_CAPTURE_URL = import.meta.env.VITE_MARKETING_LEAD_CAPTURE_URL?.trim() || "";

// Bootstrap one-liner — one command that installs zombiectl AND the skill
// bundle (host-detected) via the usezombie.sh installer. Bare-root form (no
// /install.sh path) per the M75 canonical one-liner. Shared by Hero CTA
// (clipboard payload + visible label) and OnboardingFlow step 01 — single
// source so the two surfaces cannot drift independently.
export const INSTALL_COMMAND = "curl -fsSL https://usezombie.sh | bash";

// The platform-ops install skill — step two, run inside the coding agent
// after INSTALL_COMMAND has registered the slash command. INSTALL_SKILL_SLASH
// is the bare command (OnboardingFlow step 02); INSTALL_SKILL_COMMAND is the
// Claude Code invocation the hero terminal demos and copies. Single source.
export const INSTALL_SKILL_SLASH = "/usezombie-install-platform-ops";
export const INSTALL_SKILL_COMMAND = `claude ${INSTALL_SKILL_SLASH}`;
