import { request } from "./client";
import type { TenantBilling, TenantBillingChargesResponse } from "../types";

export async function getTenantBilling(token: string): Promise<TenantBilling> {
  return request<TenantBilling>("/v1/tenants/me/billing", { method: "GET" }, token);
}

export async function listTenantBillingCharges(
  token: string,
  limit = 50,
): Promise<TenantBillingChargesResponse> {
  return request<TenantBillingChargesResponse>(
    `/v1/tenants/me/billing/charges?limit=${encodeURIComponent(String(limit))}`,
    { method: "GET" },
    token,
  );
}
