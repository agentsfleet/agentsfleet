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
// each vendor spells itself. `openai-compatible` mirrors
// OPENAI_COMPATIBLE_PROVIDER in lib/types (kept verbatim here to avoid a
// client-bundle import of the broader types module for one string). An id not
// listed is title-cased by `providerLabel`, so a new catalogue provider never
// sits as a raw slug beside the named ones in the picker.
const PROVIDER_LABELS: Readonly<Record<string, string>> = {
  anthropic: "Anthropic",
  "atlas-cloud": "Atlas Cloud",
  cohere: "Cohere",
  fireworks: "Fireworks",
  groq: "Groq",
  mistral: "Mistral",
  nearai: "NEAR AI",
  openai: "OpenAI",
  "openai-compatible": "Custom — OpenAI-compatible",
  openrouter: "OpenRouter",
  poe: "Poe",
  vercel: "Vercel",
  "vercel-ai": "Vercel AI",
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

/** Human label for a provider id; an unknown id is title-cased from its slug. */
export function providerLabel(provider: string): string {
  return PROVIDER_LABELS[provider] ?? titleCaseSlug(provider);
}
