// The model library catalogue (core.model_library), read through the
// authenticated GET /v1/models — see src/agentsfleetd/http/handlers/model_library.zig
// for the wire shape. The dashboard fetches it once per session through a
// token-minting Server Action (settings/models/actions.ts → getModelLibraryAction),
// so the token never reaches the browser. The only static client-side model
// data is the small known-models fallback list
// (settings/models/lib/known-models.ts), used when the catalogue has no rows
// for a provider. The former public unauthenticated cap.json route is retired.

import { request } from "./client";

// Route path — mirrors MODEL_LIBRARY_PATH in
// src/agentsfleetd/http/handlers/model_library.zig (shared verbatim).
const MODEL_LIBRARY_PATH = "/v1/models";

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

// Display labels for the provider ids the UI knows by name. `openai-compatible`
// mirrors OPENAI_COMPATIBLE_PROVIDER in lib/types (kept verbatim here to avoid a
// client-bundle import of the broader types module for one string). Any id not
// listed falls back to its raw slug via `providerLabel`.
const PROVIDER_LABELS: Readonly<Record<string, string>> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  "openai-compatible": "Custom — OpenAI-compatible",
};

/** Human label for a provider id; unknown ids show their raw slug. */
export function providerLabel(provider: string): string {
  return PROVIDER_LABELS[provider] ?? provider;
}

/**
 * Fetch the model library. Bearer-authed (any authenticated tenant — the route
 * carries no capability scope). Called server-side by getModelLibraryAction;
 * throws (ApiError) on a non-2xx response so callers can fall back to a
 * catalogue-free path (a free-text model field).
 */
export async function getModelLibrary(token: string): Promise<ModelLibrary> {
  return request<ModelLibrary>(MODEL_LIBRARY_PATH, { method: "GET" }, token);
}
