import { cache } from "react";
import { request } from "./client";
import type { TenantBilling, TenantBillingChargesResponse } from "../types";

export async function getTenantBilling(token: string): Promise<TenantBilling> {
  return request<TenantBilling>("/v1/tenants/me/billing", { method: "GET" }, token);
}

// Per-request deduped billing read. Multiple regions in one render (e.g. the
// dashboard's StatusTiles balance tile + ExhaustionBanner) share a single
// GET /v1/tenants/me/billing instead of each firing their own. Mirrors
// `listTenantWorkspacesCached`. Billing is tenant-scoped, so the cache key is
// the token alone — no workspace dimension.
export const getTenantBillingCached = cache(getTenantBilling);

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
