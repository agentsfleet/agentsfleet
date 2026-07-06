// Optional paste-to-fill hint: map an API-KEY prefix to a provider name so the
// wizard can pre-select the provider after a paste. This is key-FORMAT detection
// only — key formats are not in any catalogue, so they cannot come from the
// model-caps API. The provider list, the model list, and every default all come
// from the model-caps catalogue (lib/api/model_caps.ts). A null result means
// "couldn't tell from the key" → the caller falls back to the catalogue-driven
// picker. No static provider/model data lives here.
export const PROVIDER_KEY_PREFIXES: ReadonlyArray<readonly [string, string]> = [
  ["sk-ant-", "anthropic"],
  ["sk-or-", "openrouter"],
  ["fw_", "fireworks"],
  ["gsk_", "groq"],
  ["sk-", "openai"],
] as const;

export function detectProviderFromKey(apiKey: string): string | null {
  const key = apiKey.trim();
  if (!key) return null;
  for (const [prefix, provider] of PROVIDER_KEY_PREFIXES) {
    if (key.startsWith(prefix)) return provider;
  }
  return null;
}
