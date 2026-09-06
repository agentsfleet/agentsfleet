// Authenticated platform-admin client for the model library + platform default.
// Distinct from model_library.ts (the tenant-facing catalogue reader): these
// routes are platform-admin-gated and mutate core.model_library / core.platform_provider_defaults.
// Wire shape: rustd/crates/afd_api_operator/src/handler/admin/models.rs +
// .../admin/platform_keys.rs.

import { request } from "./client";

const ADMIN_MODELS_PATH = "/v1/admin/models";
const ADMIN_PLATFORM_KEYS_PATH = "/v1/admin/platform-keys";

export interface AdminModel {
  id: string;
  provider: string;
  model_id: string;
  context_cap_tokens: number;
  input_nanos_per_mtok: number;
  cached_input_nanos_per_mtok: number;
  output_nanos_per_mtok: number;
}

export interface AdminModelList {
  models: AdminModel[];
}

export interface LibraryModelInput {
  provider: string;
  model_id: string;
  context_cap_tokens: number;
  input_nanos_per_mtok: number;
  cached_input_nanos_per_mtok: number;
  output_nanos_per_mtok: number;
}

/** Caps/rates only — provider+model_id are the immutable row identity (PATCH). */
export type ModelRatesInput = Omit<LibraryModelInput, "provider" | "model_id">;

export interface PlatformDefaultInput {
  provider: string;
  source_workspace_id: string;
  model: string;
  base_url?: string;
}

// A row of core.platform_provider_defaults as the admin GET returns it. Never
// carries key material. `model` is the priced (provider, model_id) the default
// resolves to — non-null on the active row, NULL once a provider is stood down.
export interface PlatformKey {
  provider: string;
  source_workspace_id: string;
  model: string | null;
  active: boolean;
  updated_at: number;
}

export interface PlatformKeyList {
  keys: PlatformKey[];
}

export async function listAdminModels(token: string): Promise<AdminModelList> {
  return request<AdminModelList>(ADMIN_MODELS_PATH, { method: "GET" }, token);
}

export async function createAdminModel(token: string, body: LibraryModelInput): Promise<AdminModel> {
  return request<AdminModel>(ADMIN_MODELS_PATH, { method: "POST", body: JSON.stringify(body) }, token);
}

export async function updateAdminModel(
  token: string,
  id: string,
  body: ModelRatesInput,
): Promise<{ id: string; updated: boolean }> {
  return request(`${ADMIN_MODELS_PATH}/${encodeURIComponent(id)}`, { method: "PATCH", body: JSON.stringify(body) }, token);
}

export async function deleteAdminModel(token: string, id: string): Promise<void> {
  return request<void>(`${ADMIN_MODELS_PATH}/${encodeURIComponent(id)}`, { method: "DELETE" }, token);
}

export async function setPlatformDefault(
  token: string,
  body: PlatformDefaultInput,
): Promise<{ provider: string; model: string; active: boolean }> {
  return request(ADMIN_PLATFORM_KEYS_PATH, { method: "PUT", body: JSON.stringify(body) }, token);
}

export async function listPlatformKeys(token: string): Promise<PlatformKeyList> {
  return request<PlatformKeyList>(ADMIN_PLATFORM_KEYS_PATH, { method: "GET" }, token);
}

/**
 * The single active platform default, or null when none is set. The backend
 * keeps at most one row active (unique + stand-down), so a plain `find` is exact —
 * the catalogue row whose (provider, model_id) equals this row's (provider, model)
 * is the one the admin UI badges as "Default".
 */
export function activePlatformDefault(list: PlatformKeyList): PlatformKey | null {
  return list.keys.find((k) => k.active) ?? null;
}
