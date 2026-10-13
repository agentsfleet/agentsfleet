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

// Display labels for the provider ids the UI knows by name, spelled the way
// each vendor spells itself. The ids are the runner's provider registry
// (`rustd/crates/afr_providers/assets/providers.json`), so every provider a
// fleet can run on has a name here, plus the two the dashboard adds:
// `openai-compatible` mirrors OPENAI_COMPATIBLE_PROVIDER in lib/types (kept
// verbatim here to avoid a client-bundle import of the broader types module
// for one string), and `vercel-ai` names the AI gateway. An id not listed is
// title-cased by `providerLabel`, so a provider added to the registry later
// never sits as a raw slug beside the named ones in the picker.
const PROVIDER_LABELS: Readonly<Record<string, string>> = {
  aihubmix: "AIHubMix",
  anthropic: "Anthropic",
  astrai: "Astrai",
  "atlas-cloud": "Atlas Cloud",
  baichuan: "Baichuan",
  byteplus: "BytePlus",
  "byteplus-plan": "BytePlus Plan",
  cerebras: "Cerebras",
  chutes: "Chutes",
  cohere: "Cohere",
  deepseek: "DeepSeek",
  doubao: "Doubao",
  evolink: "EvoLink",
  fireworks: "Fireworks",
  glm: "GLM",
  groq: "Groq",
  huggingface: "Hugging Face",
  hunyuan: "Tencent Hunyuan",
  "kimi-code": "Kimi Code",
  minimax: "MiniMax",
  mistral: "Mistral",
  moonshot: "Moonshot AI",
  "moonshot-intl": "Moonshot AI International",
  nearai: "NEAR AI",
  novita: "Novita AI",
  nvidia: "NVIDIA",
  openai: "OpenAI",
  "openai-compatible": "Custom — OpenAI-compatible",
  opencode: "OpenCode",
  openrouter: "OpenRouter",
  ovhcloud: "OVHcloud",
  perplexity: "Perplexity",
  poe: "Poe",
  qwen: "Qwen",
  shengsuanyun: "Shengsuanyun",
  siliconflow: "SiliconFlow",
  synthetic: "Synthetic",
  telnyx: "Telnyx",
  together: "Together AI",
  venice: "Venice AI",
  vercel: "Vercel",
  "vercel-ai": "Vercel AI",
  "volcengine-plan": "Volcengine Plan",
  xai: "xAI",
};

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

/**
 * Human label for a provider id; an unknown id is title-cased from its slug.
 *
 * Own keys only: the id is server data, and a plain-object lookup would hand
 * back `Object.prototype` for `__proto__` or a function for `constructor`,
 * which React cannot render.
 */
export function providerLabel(provider: string): string {
  return Object.hasOwn(PROVIDER_LABELS, provider)
    ? (PROVIDER_LABELS[provider] as string)
    : titleCaseSlug(provider);
}
