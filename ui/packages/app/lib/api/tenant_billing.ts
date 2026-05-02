import { request } from "./client";
import type { TenantBilling, TenantBillingChargesResponse } from "../types";

export async function getTenantBilling(token: string): Promise<TenantBilling> {
  return request<TenantBilling>("/v1/tenants/me/billing", { method: "GET" }, token);
}

export async function listTenantBillingCharges(
  token: string,
  opts: { limit?: number; cursor?: string | null } = {},
): Promise<TenantBillingChargesResponse> {
  const limit = opts.limit ?? 50;
  const params = new URLSearchParams({ limit: String(limit) });
  if (opts.cursor) params.set("cursor", opts.cursor);
  return request<TenantBillingChargesResponse>(
    `/v1/tenants/me/billing/charges?${params.toString()}`,
    { method: "GET" },
    token,
  );
}
