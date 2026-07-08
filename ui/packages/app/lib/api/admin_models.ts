// Authenticated platform-admin client for the model catalogue + platform default.
// Distinct from model_caps.ts (the PUBLIC, unauthenticated cap.json reader): these
// routes are platform-admin-gated and mutate core.model_caps / core.platform_provider_defaults.
// Wire shape: src/agentsfleetd/http/handlers/admin/model_caps_admin.zig +
// .../admin/platform_keys.zig.

import { request } from "./client";

const ADMIN_MODELS_PATH = "/v1/admin/models";
const ADMIN_PLATFORM_KEYS_PATH = "/v1/admin/platform-keys";

// Rates are stored as integer nanos per million tokens (1 nano = 1e-9 USD) so the
// billing math stays in integers. The UI presents $/1M tokens — the conversion
// lives here, in one place, so every catalogue view and form agrees.
export const NANOS_PER_USD = 1_000_000_000;
export function nanosToUsdPerMtok(nanos: number): number {
  return nanos / NANOS_PER_USD;
}
export function usdPerMtokToNanos(usd: number): number {
  return Math.round(usd * NANOS_PER_USD);
}

// The provider id that opts a default into a custom OpenAI-compatible endpoint —
// mirrors OPENAI_COMPATIBLE_PROVIDER in tenant_provider_resolver.zig. Only this
// provider may carry a base_url; named providers must omit it.
export const OPENAI_COMPATIBLE_PROVIDER = "openai-compatible";

export interface AdminModel {
  uid: string;
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

export interface ModelCapInput {
  provider: string;
  model_id: string;
  context_cap_tokens: number;
  input_nanos_per_mtok: number;
  cached_input_nanos_per_mtok: number;
  output_nanos_per_mtok: number;
}

/** Caps/rates only — provider+model_id are the immutable row identity (PATCH). */
export type ModelRatesInput = Omit<ModelCapInput, "provider" | "model_id">;

export interface PlatformDefaultInput {
  provider: string;
  source_workspace_id: string;
  model: string;
  base_url?: string;
}

export async function listAdminModels(token: string): Promise<AdminModelList> {
  return request<AdminModelList>(ADMIN_MODELS_PATH, { method: "GET" }, token);
}

export async function createAdminModel(token: string, body: ModelCapInput): Promise<AdminModel> {
  return request<AdminModel>(ADMIN_MODELS_PATH, { method: "POST", body: JSON.stringify(body) }, token);
}

export async function updateAdminModel(
  token: string,
  uid: string,
  body: ModelRatesInput,
): Promise<{ uid: string; updated: boolean }> {
  return request(`${ADMIN_MODELS_PATH}/${encodeURIComponent(uid)}`, { method: "PATCH", body: JSON.stringify(body) }, token);
}

export async function deleteAdminModel(token: string, uid: string): Promise<void> {
  return request<void>(`${ADMIN_MODELS_PATH}/${encodeURIComponent(uid)}`, { method: "DELETE" }, token);
}

export async function setPlatformDefault(
  token: string,
  body: PlatformDefaultInput,
): Promise<{ provider: string; model: string; active: boolean }> {
  return request(ADMIN_PLATFORM_KEYS_PATH, { method: "PUT", body: JSON.stringify(body) }, token);
}
