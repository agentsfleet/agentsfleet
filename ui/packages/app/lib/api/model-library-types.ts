// The model library's row shape and the catalogue helpers the model pickers share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export interface LibraryModel {
  id: string;
  provider: string;
  context_cap_tokens: number;
  input_nanos_per_mtok: number;
  cached_input_nanos_per_mtok: number;
  output_nanos_per_mtok: number;
}

export interface ModelLibrary {
  version: string;
  models: LibraryModel[];
}

// The catalogue is keyed by (provider, model_id), so a model_id legitimately
// recurs across providers. These two helpers keep that keying rule in one place
// for the wizard's two distinct catalogue views: a provider-agnostic picker
// dedupes by id; a provider-scoped picker filters by provider.

/** One entry per model_id (last occurrence wins) — for a provider-agnostic picker. */
export function uniqueModelIds(models: LibraryModel[]): LibraryModel[] {
  return Array.from(new Map(models.map((m) => [m.id, m])).values());
}

/** Models belonging to one provider — unique by id within that provider (the PK). */
export function modelsForProvider(models: LibraryModel[], provider: string): LibraryModel[] {
  return models.filter((m) => m.provider === provider);
}

/**
 * Distinct provider ids the catalogue serves, first-occurrence order preserved.
 * The switch list unions these with the providers the workspace has stored keys
 * for, so an operator can add a key for a catalogue provider they haven't
 * configured yet.
 */
export function uniqueProviders(models: LibraryModel[]): string[] {
  return Array.from(new Set(models.map((m) => m.provider)));
}

// The provider ids whose vendor spelling title case would miss, spelled the
// way each vendor spells itself. The ids are the runner's provider registry
// (`rustd/crates/afr_providers/assets/providers.json`), plus the two the
// dashboard adds: `openai-compatible` mirrors OPENAI_COMPATIBLE_PROVIDER in
// lib/types (kept verbatim here to avoid a client-bundle import of the broader
// types module for one string), and `vercel-ai` names the AI gateway. Every
// other id, `anthropic` or `atlas-cloud` say, reads right title-cased by
// `providerLabel`, so a provider added to the registry later never sits as a
// raw slug beside the named ones in the picker. A Map, so a server-sent id
// such as `__proto__` can never reach an inherited property.
const PROVIDER_LABELS: ReadonlyMap<string, string> = new Map([
  ["aihubmix", "AIHubMix"],
  ["byteplus", "BytePlus"],
  ["byteplus-plan", "BytePlus Plan"],
  ["deepseek", "DeepSeek"],
  ["evolink", "EvoLink"],
  ["glm", "GLM"],
  ["huggingface", "Hugging Face"],
  ["hunyuan", "Tencent Hunyuan"],
  ["minimax", "MiniMax"],
  ["moonshot", "Moonshot AI"],
  ["moonshot-intl", "Moonshot AI International"],
  ["nearai", "NEAR AI"],
  ["novita", "Novita AI"],
  ["nvidia", "NVIDIA"],
  ["openai", "OpenAI"],
  ["openai-compatible", "Custom — OpenAI-compatible"],
  ["opencode", "OpenCode"],
  ["openrouter", "OpenRouter"],
  ["ovhcloud", "OVHcloud"],
  ["siliconflow", "SiliconFlow"],
  ["together", "Together AI"],
  ["venice", "Venice AI"],
  ["vercel-ai", "Vercel AI"],
  ["xai", "xAI"],
]);

const SLUG_SEPARATOR = /[-_]+/;
const WORD_SEPARATOR = " ";

/** `atlas-cloud` → `Atlas Cloud`: each hyphen- or underscore-separated word capitalised. */
function titleCaseSlug(slug: string): string {
  return slug
    .split(SLUG_SEPARATOR)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(WORD_SEPARATOR);
}

/** Human label for a provider id; an id the table does not name is title-cased from its slug. */
export function providerLabel(provider: string): string {
  return PROVIDER_LABELS.get(provider) ?? titleCaseSlug(provider);
}
