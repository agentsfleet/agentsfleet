import { request } from "./client";

// Workspace credential vault — the plaintext is an opaque JSON object
// whose top-level keys are the field names a skill references via
// `${secrets.<name>.<field>}`. The runtime never returns the data; reads
// here are name + created_at only.

export interface CredentialSummary {
  name: string;
  /** Epoch milliseconds — `vault.secrets.created_at`, serialized as int64. */
  created_at: number;
}

/**
 * The decrypted credential body the vault stores (never returned on read). The
 * shape is open (`[key: string]: unknown`) because a SKILL.md references
 * arbitrary fields by name, but the self-managed model-provider fields are
 * typed so the own-key + custom-endpoint write paths can't drift: `provider` +
 * optional `base_url` (required iff `provider === OPENAI_COMPATIBLE_PROVIDER`,
 * https + SSRF-validated server-side) alongside `api_key` / `model`.
 */
export interface CredentialData {
  provider?: string;
  api_key?: string;
  model?: string;
  base_url?: string;
  [key: string]: unknown;
}

export interface CredentialListResponse {
  credentials: CredentialSummary[];
}

export async function listCredentials(
  workspaceId: string,
  token: string,
): Promise<CredentialListResponse> {
  return request<CredentialListResponse>(
    `/v1/workspaces/${workspaceId}/credentials`,
    { method: "GET" },
    token,
  );
}

export async function createCredential(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
  token: string,
): Promise<{ name: string }> {
  return request<{ name: string }>(
    `/v1/workspaces/${workspaceId}/credentials`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

export async function deleteCredential(
  workspaceId: string,
  name: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/credentials/${encodeURIComponent(name)}`,
    { method: "DELETE" },
    token,
  );
}
