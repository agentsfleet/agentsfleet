// Config-driven vault taxonomy (RULE CFG). The Credentials vault renders three
// kinds in a fixed order — model providers first (they power the own-key model
// path), custom secrets next (named JSON objects the SKILL.md reads), and
// integrations last. The strings live here
// once so the page renderer, the IntegrationsComingSoon component, and the tests
// share the exact same labels (RULE UFS) instead of restating them.

export const VAULT_KIND = {
  providers: "providers",
  custom: "custom",
  integrations: "integrations",
} as const;

export type VaultKind = (typeof VAULT_KIND)[keyof typeof VAULT_KIND];

export type VaultKindMeta = {
  kind: VaultKind;
  label: string;
  blurb: string;
  examples: string;
};

// Order is load-bearing: providers → custom → integrations.
export const VAULT_KINDS: readonly VaultKindMeta[] = [
  {
    kind: VAULT_KIND.providers,
    label: "Model providers",
    blurb: "Own-key model keys.",
    examples: "anthropic · openai · custom endpoint",
  },
  {
    kind: VAULT_KIND.custom,
    label: "Custom secrets",
    blurb: "Named JSON for fleets.",
    examples: "STRIPE_API_KEY · INTERNAL_WEBHOOK",
  },
  {
    kind: VAULT_KIND.integrations,
    label: "Integrations",
    blurb: "Tool tokens for fleets.",
    examples: "github · zoho · slack · …",
  },
] as const;
