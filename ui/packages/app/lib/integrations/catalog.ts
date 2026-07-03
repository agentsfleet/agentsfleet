export const INTEGRATION_AUTH = {
  // GitHub: one-click browser OAuth App install — connect once, the broker mints
  // installation tokens on demand. No token is ever pasted or stored by the user.
  appConnect: "app_connect",
  // Slack: browser OAuth connect (M106) — the callback vaults the bot token; no
  // paste. A resident channel bot answers @mentions in-thread.
  oauthConnect: "oauth_connect",
  // Zoho: paste a token into the vault for now (custom-secret bridge until it
  // grows a native connector).
  vaultSecret: "vault_secret",
  // Jira / Linear / Grafana: on the roadmap, no bridge yet. Shown as
  // "Not connected" with a Request-access signal so demand is captured.
  comingSoon: "coming_soon",
} as const;

export const ZOHO_TOKEN_SECRET = "ZOHO_TOKEN";

export const INTEGRATION_CATALOG = [
  {
    id: "github",
    name: "GitHub",
    auth: INTEGRATION_AUTH.appConnect,
    description: "Run fleets on issues and pull requests.",
  },
  {
    id: "zoho",
    name: "Zoho",
    auth: INTEGRATION_AUTH.vaultSecret,
    requiredSecret: ZOHO_TOKEN_SECRET,
    description: "Summarize Sprints, act on Desk tickets.",
  },
  {
    id: "slack",
    name: "Slack",
    auth: INTEGRATION_AUTH.oauthConnect,
    description: "Mention a fleet in a channel; it answers in-thread.",
  },
  {
    id: "jira",
    name: "Jira",
    auth: INTEGRATION_AUTH.comingSoon,
    description: "Triage and comment on Jira issues.",
  },
  {
    id: "linear",
    name: "Linear",
    auth: INTEGRATION_AUTH.comingSoon,
    description: "Pick up and update Linear issues.",
  },
  {
    id: "grafana",
    name: "Grafana",
    auth: INTEGRATION_AUTH.comingSoon,
    description: "Investigate alerts straight from Grafana.",
  },
] as const;

export type Integration = (typeof INTEGRATION_CATALOG)[number];
