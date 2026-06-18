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

// Self-hosted Clerk <Waitlist>, served by the app at /waitlist
// (ui/packages/app app/(auth)/waitlist). "Get early access" links here (hero,
// topbar, pricing usage tier) rather than the Clerk-hosted Account Portal —
// self-hosting lets the app hide the "Already have access? Sign in" footer row
// and apply its mint/dark theming, neither reliable on the hosted portal. The
// app's /waitlist route is public (proxy.ts), so it renders signed-out; the
// dashboard owner still enables Waitlist mode in Clerk for signups to be
// accepted. PROD/dev split mirrors APP_BASE_URL; env-overridable via
// VITE_WAITLIST_URL.
export const WAITLIST_URL = import.meta.env.VITE_WAITLIST_URL?.trim() || (
  import.meta.env.PROD
    ? "https://app.agentsfleet.net/waitlist"
    : "https://app.dev.agentsfleet.net/waitlist"
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
