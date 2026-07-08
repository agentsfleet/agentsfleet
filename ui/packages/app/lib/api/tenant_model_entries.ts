import { request } from "./client";
import type {
  TenantModelEntryList,
  TenantModelEntryWriteResult,
} from "../types";

// GET/POST/PATCH/DELETE /v1/tenants/me/models — see
// src/agentsfleetd/http/handlers/tenant_model_entries.zig for the wire
// contract. `api_key` never appears in any response; each entry carries only
// `has_key` plus the metadata joined from its referenced secret.

export async function listTenantModelEntries(token: string): Promise<TenantModelEntryList> {
  return request<TenantModelEntryList>("/v1/tenants/me/models", { method: "GET" }, token);
}

export async function createTenantModelEntry(
  body: { model_id: string; secret_ref: string },
  token: string,
): Promise<TenantModelEntryWriteResult> {
  return request<TenantModelEntryWriteResult>(
    "/v1/tenants/me/models",
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

export async function updateTenantModelEntry(
  id: string,
  body: { model_id: string },
  token: string,
): Promise<TenantModelEntryWriteResult> {
  return request<TenantModelEntryWriteResult>(
    `/v1/tenants/me/models/${encodeURIComponent(id)}`,
    { method: "PATCH", body: JSON.stringify(body) },
    token,
  );
}

export async function deleteTenantModelEntry(id: string, token: string): Promise<void> {
  return request<void>(
    `/v1/tenants/me/models/${encodeURIComponent(id)}`,
    { method: "DELETE" },
    token,
  );
}
