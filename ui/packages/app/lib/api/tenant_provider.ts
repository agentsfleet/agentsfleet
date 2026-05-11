import { request } from "./client";
import { PROVIDER_MODE, type TenantProvider } from "../types";

// GET/PUT/DELETE /v1/tenants/me/provider — see src/http/handlers/tenant_provider.zig
// for the wire contract. The api_key is never returned in responses; this
// helper only ever surfaces the resolved metadata (mode, provider, model,
// credential_ref, context_cap_tokens).

export async function getTenantProvider(token: string): Promise<TenantProvider> {
  return request<TenantProvider>("/v1/tenants/me/provider", { method: "GET" }, token);
}

export async function setTenantProviderSelfManaged(
  body: { credential_ref: string; model?: string },
  token: string,
): Promise<TenantProvider> {
  return request<TenantProvider>(
    "/v1/tenants/me/provider",
    {
      method: "PUT",
      body: JSON.stringify({
        mode: PROVIDER_MODE.self_managed,
        credential_ref: body.credential_ref,
        model: body.model,
      }),
    },
    token,
  );
}

export async function resetTenantProvider(token: string): Promise<TenantProvider> {
  return request<TenantProvider>("/v1/tenants/me/provider", { method: "DELETE" }, token);
}
