import { request } from "./client";
import type { TenantBilling } from "../types";

export async function getTenantBilling(token: string): Promise<TenantBilling> {
  return request<TenantBilling>("/v1/tenants/me/billing", { method: "GET" }, token);
}
