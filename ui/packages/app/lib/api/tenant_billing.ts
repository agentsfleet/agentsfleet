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

// The rail's credits and the fleet header share one billing read per request.
// Billing is tenant-scoped, so the token alone identifies the cache entry.
export const getTenantBillingCached = cache(getTenantBilling);
