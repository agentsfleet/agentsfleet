const fromEnv = import.meta.env.VITE_APP_BASE_URL?.trim();

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
