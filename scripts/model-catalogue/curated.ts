import type { Model } from "./catalogue";

const MINIMAX_FREE_IDENTITY = "openrouter/minimax/minimax-m3.1-flash:free";

/** Explicitly selected catalogue identities. */
export const CURATED_IDENTITIES = [
  "anthropic/claude-fable-5-1", "anthropic/claude-opus-5-5", "anthropic/claude-sonnet-5-5", "anthropic/claude-haiku-5-5",
  "openai/gpt-6.1-sol", "openai/gpt-6-astra", "openai/gpt-6-luna", "openai/gpt-5.6-terra",
  "fireworks/accounts/fireworks/models/glm-5p3-flash", "fireworks/accounts/fireworks/models/kimi-k3",
  "fireworks/accounts/fireworks/models/deepseek-v4p1-flash",
  "openrouter/meta/muse-wonder", "openrouter/google/gemini-4-argon", "openrouter/qwen/qwen3.8-max-0902",
  MINIMAX_FREE_IDENTITY,
  "groq/openai/gpt-oss-120b", "mistral/mistral-large-4", "mistral/mistral-small-2603", "gemini/gemini-4-argon",
] as const;
const curated = new Set<string>(CURATED_IDENTITIES);
const FREE_MODELS = new Set([MINIMAX_FREE_IDENTITY]);
type Provider = Model["provider"];
const PRICE_HOSTS: Record<Provider, string> = {
  anthropic: "platform.claude.com", openai: "developers.openai.com", fireworks: "fireworks.ai",
  openrouter: "openrouter.ai", groq: "console.groq.com", mistral: "docs.mistral.ai", gemini: "ai.google.dev",
};

export function isCuratedModel(model: { provider: Provider; model_name: string }): boolean {
  return curated.has(`${model.provider}/${model.model_name}`);
}

export function isFreeModel(model: { provider: Provider; model_name: string }): boolean {
  return FREE_MODELS.has(`${model.provider}/${model.model_name}`);
}

export function isOfficialPriceSource(provider: Provider, source: string): boolean {
  return URL.canParse(source) && new URL(source).hostname === PRICE_HOSTS[provider];
}

/** Completeness is checked before an operator can prune the live catalogue. */
export function assertCompleteCatalogue(models: readonly { provider: string; model_name: string }[]): void {
  const present = new Set(models.map((model) => `${model.provider}/${model.model_name}`));
  const missing = CURATED_IDENTITIES.filter((identity) => !present.has(identity));
  if (missing.length || present.size !== CURATED_IDENTITIES.length || models.length !== present.size)
    throw new Error(`Catalogue must contain exactly the curated identities; missing: ${missing.join(", ")}`);
}
