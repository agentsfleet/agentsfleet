// Authenticated platform-admin client for the model library + platform default.
// Distinct from model_library.ts (the tenant-facing catalogue reader): these
// routes are platform-admin-gated and manage model.platform_models and its prices.
// Wire shape: rustd/crates/afd_api_operator/src/handler/admin/models.rs +
// .../admin/platform_keys/mod.rs.

import { z } from "zod";
import { request } from "./client";
import { adminModelSchema, adminModelListSchema, modelSettingsSchema, boundedText, CATALOGUE_LIMITS } from "./catalogue-types";

const ADMIN_MODELS_PATH = "/v1/admin/models";
const ADMIN_PLATFORM_KEYS_PATH = "/v1/admin/platform-keys";
const createModelSchema = modelSettingsSchema.extend({
  provider: boundedText(CATALOGUE_LIMITS.provider).pipe(z.string().min(1)),
  model_id: boundedText(CATALOGUE_LIMITS.modelId).pipe(z.string().min(1)),
});

const updateModelResultSchema = z.object({ id: z.string(), updated: z.boolean() });

export type AdminModel = z.infer<typeof adminModelSchema>;
export type AdminModelList = z.infer<typeof adminModelListSchema>;
export type ModelRatesInput = z.infer<typeof modelSettingsSchema>;
export type LibraryModelInput = z.infer<typeof createModelSchema>;
export type { ModelInference } from "./catalogue-types";

export interface PlatformDefaultInput {
  provider: string;
  source_workspace_id: string;
  model: string;
  base_url?: string;
}

// A platform provider default as the admin GET returns it. Never
// carries key material. `model` is the priced (provider, model_id) the default
// resolves to — non-null on the active row, NULL once a provider is stood down.
export interface PlatformKey {
  provider: string;
  source_workspace_id: string;
  model: string | null;
  active: boolean;
  updated_at: string;
}

export interface PlatformKeyList {
  keys: PlatformKey[];
}

export async function listAdminModels(token: string): Promise<AdminModelList> {
  return adminModelListSchema.parse(await request<unknown>(ADMIN_MODELS_PATH, { method: "GET" }, token));
}

export async function createAdminModel(token: string, body: LibraryModelInput): Promise<AdminModel> {
  const parsed = createModelSchema.parse(body);
  return adminModelSchema.parse(await request<unknown>(ADMIN_MODELS_PATH, { method: "POST", body: JSON.stringify(parsed) }, token));
}

export async function updateAdminModel(
  token: string,
  id: string,
  body: ModelRatesInput,
): Promise<{ id: string; updated: boolean }> {
  return updateModelResultSchema.parse(await request<unknown>(`${ADMIN_MODELS_PATH}/${encodeURIComponent(id)}`,
    { method: "PATCH", body: JSON.stringify(modelSettingsSchema.parse(body)) }, token));
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
