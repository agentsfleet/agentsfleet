// A small, static, client-only list of common model names per well-known
// provider — purely an autocomplete convenience, NOT the priced/billing
// catalogue (core.model_library, admin-managed via /admin/models). Used only
// as a fallback when the admin catalogue has zero rows for a given provider;
// falls back further to free text when even this list doesn't cover the
// provider. The keys are the well-known hosted providers; the
// custom/openai-compatible slot never uses this list (its model is always
// free text).
export const KNOWN_MODELS: Readonly<Record<string, readonly string[]>> = {
  anthropic: ["claude-sonnet-5-5", "claude-opus-5-5", "claude-fable-5-1", "claude-haiku-5-5"],
  openai: ["gpt-6.1-sol", "gpt-6-astra", "gpt-6-luna"],
  fireworks: [
    "accounts/fireworks/models/kimi-k3",
    "accounts/fireworks/models/glm-5p3",
    "accounts/fireworks/models/deepseek-v4p1-flash",
  ],
  groq: ["openai/gpt-oss-120b", "openai/gpt-oss-20b"],
  openrouter: ["anthropic/claude-sonnet-5.5", "anthropic/claude-opus-5.5"],
} as const;

/** Known model names for one provider; empty when the provider isn't listed. */
export function knownModelsFor(provider: string): readonly string[] {
  return KNOWN_MODELS[provider] ?? [];
}
