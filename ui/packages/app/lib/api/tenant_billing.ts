import { cache } from "react";
import { z } from "zod";
import { request } from "./client";
import { Instant } from "./decode";
import { signedNanosSchema } from "./nanos";
import type { TenantBilling } from "../types";

const BillingSchema = z.object({
  balance_nanos: signedNanosSchema, updated_at: Instant, is_exhausted: z.boolean(), exhausted_at: Instant.nullable(),
});
export async function getTenantBilling(token: string): Promise<TenantBilling> {
  return BillingSchema.parse(await request<unknown>("/v1/tenants/me/billing", { method: "GET" }, token));
}

// Per-request deduped billing read. Multiple regions in one render (e.g. the
// Wall's ExhaustionBanner and any billing-linked copy) share a single
// GET /v1/tenants/me/billing instead of each firing their own. Mirrors
// `listTenantWorkspacesCached`. Billing is tenant-scoped, so the cache key is
// the token alone — no workspace dimension.
export const getTenantBillingCached = cache(getTenantBilling);
