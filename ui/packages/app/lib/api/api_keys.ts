import { request } from "./client";

import { walkList } from "./list-walk";

import { QUERY_STARTING_AFTER } from "./runners";
import { DEFAULT_SORT } from "./api-keys-types";
import type { ApiKeySort } from "./api-keys-types";

// Tenant API keys are tenant-scoped: every endpoint filters by the principal's
// tenant_id server-side, so none of these calls take a workspace id (unlike
// credentials). The raw `key` is secret material from the moment it crosses the
// network boundary — it is never logged, echoed, or persisted past the reveal.

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
  total: number | null;
  next_cursor: string | null;
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

// A tenant's keys are human-created and number in the low tens, so the client
// exposes no paging controls: the list is complete by construction, walking
// next_cursor until the server reports the end (the shared list-walk bound
// guards against a runaway cursor).
export async function listApiKeys(token: string, sort: ApiKeySort = DEFAULT_SORT): Promise<ApiKeyListResponse> {
  const walked = await walkList<ApiKeyRow>("API key list", (cursor) => {
    const qs = new URLSearchParams({ sort });
    if (cursor !== null) qs.set(QUERY_STARTING_AFTER, cursor);
    return request<ApiKeyListResponse>(`/v1/api-keys?${qs.toString()}`, { method: "GET" }, token);
  });
  return { items: walked.items, total: walked.total, next_cursor: null };
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
