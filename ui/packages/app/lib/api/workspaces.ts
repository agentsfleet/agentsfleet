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
