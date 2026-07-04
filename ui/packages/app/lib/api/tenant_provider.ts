import { request } from "./client";
import { PROVIDER_MODE, type TenantProvider } from "../types";

// GET/PUT/DELETE /v1/tenants/me/provider — see src/http/handlers/tenant_provider.zig
// for the wire contract. The api_key is never returned in responses; this
// helper only ever surfaces the resolved metadata (mode, provider, model,
// secret_ref, context_cap_tokens).

export async function getTenantProvider(token: string): Promise<TenantProvider> {
  return request<TenantProvider>("/v1/tenants/me/provider", { method: "GET" }, token);
}

export async function setTenantProviderSelfManaged(
  body: { secret_ref: string; model?: string },
  token: string,
): Promise<TenantProvider> {
  return request<TenantProvider>(
    "/v1/tenants/me/provider",
    {
      method: "PUT",
      body: JSON.stringify({
        mode: PROVIDER_MODE.self_managed,
        secret_ref: body.secret_ref,
        model: body.model,
      }),
    },
    token,
  );
}

export async function resetTenantProvider(token: string): Promise<TenantProvider> {
  return request<TenantProvider>("/v1/tenants/me/provider", { method: "DELETE" }, token);
}
