import { cache } from "react";
import { z } from "zod";
import { request } from "./client";
import { Instant } from "./decode";
import { nanosSchema, signedNanosSchema } from "./nanos";
import { CHARGE_TYPE } from "../types";
import type { TenantBilling, TenantBillingChargesResponse } from "../types";

const BillingSchema = z.object({
  balance_nanos: signedNanosSchema, updated_at: Instant, is_exhausted: z.boolean(), exhausted_at: Instant.nullable(),
});
const Count = z.number().int().nonnegative().nullable();
const ChargesSchema = z.object({
  items: z.array(z.object({
    id: z.string(), tenant_id: z.string(), workspace_id: z.string(),
    fleet_id: z.string().nullable(), fleet_name: z.string().nullable(), event_id: z.string().nullable(),
    authoring_attempt_id: z.string().nullable(), charge_type: z.enum(CHARGE_TYPE),
    posture: z.enum(["platform", "self_managed"]), model: z.string(), credit_deducted_nanos: nanosSchema,
    token_count_input: Count, token_count_output: Count, wall_ms: Count, recorded_at: Instant,
  })),
  next_cursor: z.string().nullable(),
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

export async function listTenantBillingCharges(
  token: string,
  opts: { limit?: number; cursor?: string | null } = {},
): Promise<TenantBillingChargesResponse> {
  const limit = opts.limit ?? 50;
  const params = new URLSearchParams({ limit: String(limit) });
  if (opts.cursor) params.set("cursor", opts.cursor);
  return ChargesSchema.parse(await request<unknown>(
    `/v1/tenants/me/billing/charges?${params.toString()}`,
    { method: "GET" },
    token,
  ));
}
