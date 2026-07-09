// The public model→cap catalogue plus the global, non-secret client config,
// served unauthenticated at /_um/<key>/cap.json. The model list, per-model
// context caps + token rates, and the global rates/billing block all come from
// here — see src/agentsfleetd/http/handlers/model_caps.zig for the wire contract.
// Everything catalogue-shaped is fetched from this endpoint; the only static
// client-side model data is the small known-models fallback list
// (settings/models/lib/known-models.ts), used when the catalogue has no rows
// for a provider.

import { BASE } from "./client";

// Public path obfuscator — NOT a secret. Shipped in agentsfleet, the install-skill,
// and the agentsfleetd binary; mirrors MODEL_CAPS_PATH_KEY in model_caps.zig. It only
// deflects opportunistic crawlers — the catalogue itself is public.
const CAP_JSON_PATH_KEY = "da5b6b3810543fe108d816ee972e4ff8"; // gitleaks:allow — public path obfuscator, not a credential
const CAP_JSON_PATH = `/_um/${CAP_JSON_PATH_KEY}/cap.json`;

// The catalogue changes only when model rows are updated in the DB, so the fetch
// below opts back into ISR even though the Models page is force-dynamic.
const CAP_JSON_REVALIDATE_SECONDS = 300;

export interface ModelCap {
  id: string;
  provider: string;
  context_cap_tokens: number;
  input_nanos_per_mtok: number;
  cached_input_nanos_per_mtok: number;
  output_nanos_per_mtok: number;
}

export interface CapRates {
  run_nanos_per_sec: number;
  event_nanos: number;
}

export interface CapBilling {
  starter_credit_nanos: number;
  free_trial_end_ms: number;
  free_trial_stage_nanos: number;
}

export interface CapJson {
  version: string;
  models: ModelCap[];
  rates: CapRates;
  billing: CapBilling;
}

// The catalogue is keyed by (provider, model_id), so a model_id legitimately
// recurs across providers. These two helpers keep that keying rule in one place
// for the wizard's two distinct catalogue views: a provider-agnostic picker
// dedupes by id; a provider-scoped picker filters by provider.

/** One entry per model_id (last occurrence wins) — for a provider-agnostic picker. */
export function uniqueModelIds(models: ModelCap[]): ModelCap[] {
  return Array.from(new Map(models.map((m) => [m.id, m])).values());
}

/** Models belonging to one provider — unique by id within that provider (the PK). */
export function modelsForProvider(models: ModelCap[], provider: string): ModelCap[] {
  return models.filter((m) => m.provider === provider);
}

/**
 * Distinct provider ids the catalogue serves, first-occurrence order preserved.
 * The switch list unions these with the providers the workspace has stored keys
 * for, so an operator can add a key for a catalogue provider they haven't
 * configured yet.
 */
export function uniqueProviders(models: ModelCap[]): string[] {
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
 * Fetch the public model catalogue + global config. Unauthenticated — no Bearer
 * token (the document is global, non-secret). Server-fetched by the Models page
 * and passed into the wizard as props. Throws on a non-2xx response so callers
 * can fall back to a catalogue-free path (a free-text model field).
 */
export async function getModelCaps(): Promise<CapJson> {
  const res = await fetch(`${BASE}${CAP_JSON_PATH}`, {
    method: "GET",
    headers: { Accept: "application/json" },
    // Opt back into ISR under the page's force-dynamic so the Models page
    // doesn't make a cold catalogue round-trip on every server render.
    next: { revalidate: CAP_JSON_REVALIDATE_SECONDS },
  });
  if (!res.ok) {
    throw new Error(`cap.json fetch failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as CapJson;
}
