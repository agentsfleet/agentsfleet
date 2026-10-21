import type { Model } from "./catalogue";
import allowlist from "../model-library-allowlist.json";

const MINIMAX_FREE_IDENTITY = "openrouter/minimax/minimax-m3.1-flash:free";

const curated = new Set(allowlist.models.map((model) => `${model.provider}/${model.model_name}`));
export const catalogueModelNames: ReadonlySet<string> = new Set(allowlist.models.map((model) => model.model_name));
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
