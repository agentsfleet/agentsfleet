import { request } from "./client";

export type TenantWorkspace = {
  id: string;
  name: string | null;
  created_at: number;
};

export type TenantWorkspaceListResponse = {
  items: TenantWorkspace[];
  total: number;
};

export type CreateWorkspaceResponse = {
  workspace_id: string;
  name: string;
  request_id?: string;
};

// GET /v1/tenants/me/workspaces — every workspace the caller's tenant owns.
// Backend reads tenant_id from the JWT principal; frontend never passes it.
export async function listTenantWorkspaces(
  token: string,
): Promise<TenantWorkspaceListResponse> {
  return request<TenantWorkspaceListResponse>(
    "/v1/tenants/me/workspaces",
    { method: "GET" },
    token,
  );
}

// POST /v1/workspaces — body { name? }. A blank/omitted name lets the server
// pick a Heroku-style name (parity with signup + `zombiectl workspace add`).
// Backend reads tenant_id from the JWT principal.
export async function createTenantWorkspace(
  token: string,
  body: { name?: string } = {},
): Promise<CreateWorkspaceResponse> {
  return request<CreateWorkspaceResponse>(
    "/v1/workspaces",
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}
