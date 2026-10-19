import { PRICING_UNIT, type Catalogue, type Model, type Price } from "./catalogue";

export const MODEL_NAME = "accounts/fireworks/models/glm-5p3-flash";
export const VERIFIED_AT = "2026-10-18T12:00:00Z";
export const PRICE_SOURCE = "https://fireworks.ai/models/fireworks/glm-5p3-flash";
export const APPLY_FLAG = "--apply";
export const HAIKU_PROMPT_THRESHOLD = 100_000;

export function priceFixture(overrides: Partial<Price> = {}): Price {
  return { effective_from: VERIFIED_AT, effective_until: null, prompt_tokens_above: 0,
    input: "0.15", cached_input: "0.03", output: "0.5", cache_write: null,
    cache_write_5m: null, cache_write_1h: null, source_url: PRICE_SOURCE,
    verified_at: VERIFIED_AT, conditions: "Standard synchronous inference.", ...overrides };
}

export function modelFixture(overrides: Partial<Model> = {}): Model {
  return { provider: "fireworks", model_name: MODEL_NAME, display_name: "GLM 5.3 Flash", advanced: false,
    context: { tokens: 1_048_576, source_url: `https://api.fireworks.ai/v1/${MODEL_NAME}#contextLength`, verified_at: VERIFIED_AT },
    inference: { api: "chat", base_url: "https://api.fireworks.ai/inference/v1", max_output_tokens: 943718 },
    context_unverified_reason: null, prices: [priceFixture()], ...overrides };
}

export function catalogueFixture(models: Model[] = [modelFixture()]): Catalogue {
  return { verified_at: VERIFIED_AT, pricing_unit: PRICING_UNIT, models,
    ollama: { display_name: "Ollama", pricing: "unknown", models_source: "configured server /api/tags", endpoint_kinds: ["local", "cloud"] } };
}
