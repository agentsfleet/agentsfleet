export const INTEGRATION_STATUS = {
  native: "native",
  customSecret: "custom_secret",
} as const;

export const INTEGRATION_AUTH = {
  vaultSecret: "vault_secret",
} as const;

export const GITHUB_TOKEN_SECRET = "GITHUB_TOKEN";
export const ZOHO_TOKEN_SECRET = "ZOHO_TOKEN";
export const SLACK_BOT_TOKEN_SECRET = "SLACK_BOT_TOKEN";

export const INTEGRATION_CATALOG = [
  {
    id: "github",
    name: "GitHub",
    status: INTEGRATION_STATUS.native,
    auth: INTEGRATION_AUTH.vaultSecret,
    requiredSecret: GITHUB_TOKEN_SECRET,
    description: "Run fleets on issues and pull requests.",
  },
  {
    id: "zoho",
    name: "Zoho",
    status: INTEGRATION_STATUS.customSecret,
    auth: INTEGRATION_AUTH.vaultSecret,
    requiredSecret: ZOHO_TOKEN_SECRET,
    description: "Summarize Sprints, act on Desk tickets.",
  },
  {
    id: "slack",
    name: "Slack",
    status: INTEGRATION_STATUS.customSecret,
    auth: INTEGRATION_AUTH.vaultSecret,
    requiredSecret: SLACK_BOT_TOKEN_SECRET,
    description: "Mention a fleet in channels; post run results.",
  },
] as const;

export type Integration = (typeof INTEGRATION_CATALOG)[number];
