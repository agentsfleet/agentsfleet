import { request } from "./client";

// Workspace credential vault — the plaintext is an opaque JSON object
// whose top-level keys are the field names a skill references via
// `${secrets.<name>.<field>}`. The runtime never returns the data; reads
// here are name + created_at only.

export interface CredentialSummary {
  name: string;
  created_at: string;
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
