import { request } from "./client";

// Tenant API keys are tenant-scoped: every endpoint filters by the principal's
// tenant_id server-side, so none of these calls take a workspace id (unlike
// credentials). The raw `key` is secret material from the moment it crosses the
// network boundary — it is never logged, echoed, or persisted past the reveal.

// Mirrors the Zig handler's constants verbatim (src/http/handlers/api_keys):
// tenant.zig isValidKeyName (1-64 chars, alnum + - + _) and MAX_DESC_LEN,
// list.zig DEFAULT_PAGE_SIZE / MAX_PAGE_SIZE and the sort allowlist.
export const KEY_PREFIX = "zmb_t_";
export const KEY_NAME_REGEX = /^[A-Za-z0-9_-]{1,64}$/;
export const KEY_NAME_MAX = 64;
export const DESCRIPTION_MAX = 256;
export const DEFAULT_PAGE_SIZE = 25;
export const MAX_PAGE_SIZE = 100;

export const API_KEY_SORTS = ["-created_at", "created_at", "-key_name", "key_name"] as const;
export type ApiKeySort = (typeof API_KEY_SORTS)[number];
export const DEFAULT_SORT: ApiKeySort = "-created_at";

export interface ApiKeyRow {
  id: string;
  key_name: string;
  active: boolean;
  /** Epoch milliseconds. */
  created_at: number;
  /** Epoch milliseconds, or null when the key has never authenticated a call. */
  last_used_at: number | null;
  /** Epoch milliseconds, or null while the key is still active. */
  revoked_at: number | null;
}

export interface ApiKeyListResponse {
  items: ApiKeyRow[];
  total: number;
  page: number;
  page_size: number;
}

/** The mint response — `key` is the raw secret, returned exactly once. */
export interface CreatedApiKey {
  id: string;
  key_name: string;
  key: string;
  created_at: number;
}

export interface RevokedApiKey {
  id: string;
  active: boolean;
  revoked_at: number;
}

export interface ListParams {
  page?: number;
  page_size?: number;
  sort?: ApiKeySort;
}

export async function listApiKeys(token: string, params: ListParams = {}): Promise<ApiKeyListResponse> {
  const qs = new URLSearchParams({
    page: String(params.page ?? 1),
    page_size: String(params.page_size ?? DEFAULT_PAGE_SIZE),
    sort: params.sort ?? DEFAULT_SORT,
  });
  return request<ApiKeyListResponse>(`/v1/api-keys?${qs.toString()}`, { method: "GET" }, token);
}

export async function createApiKey(
  token: string,
  body: { key_name: string; description?: string },
): Promise<CreatedApiKey> {
  return request<CreatedApiKey>(`/v1/api-keys`, { method: "POST", body: JSON.stringify(body) }, token);
}

export async function revokeApiKey(token: string, id: string): Promise<RevokedApiKey> {
  return request<RevokedApiKey>(
    `/v1/api-keys/${encodeURIComponent(id)}`,
    { method: "PATCH", body: JSON.stringify({ active: false }) },
    token,
  );
}

export async function deleteApiKey(token: string, id: string): Promise<void> {
  return request<void>(`/v1/api-keys/${encodeURIComponent(id)}`, { method: "DELETE" }, token);
}
