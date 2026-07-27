import { request } from "./client";
import type {
  TenantModelEntryList,
  TenantModelEntryWriteResult,
} from "../types";

// GET/POST/PATCH/DELETE /v1/tenants/me/models — see
// src/agentsfleetd/http/handlers/tenant_model_entries.zig for the wire
// contract. `api_key` never appears in any response; each entry carries only
// `has_key` plus the metadata joined from its referenced secret.

// Rows per request. The endpoint pages at 50 by default and rejects a `limit`
// above 100 (`UZ-LIBRARY-003`), so asking for the maximum gives the largest
// window one round-trip can buy.
const REGISTRY_PAGE_LIMIT = 100;

/** One wire page: `models` is that page alone, `next_cursor` null on the last. */
type TenantModelEntryPage = TenantModelEntryList & {
  total: number | null;
  next_cursor: string | null;
};

/** A page plus the cursor and total the caller needs to retain and disclose. */
export type TenantModelEntryPageResult = TenantModelEntryList & {
  next_cursor: string | null;
  total: number | null;
};

// ONE page per call. This replaced a walk that followed
// `next_cursor` to exhaustion on every ordinary visit to the Models page.
//
// That walk existed for a real reason — reading a single page would drop every
// entry past the server's page size *silently*, with nothing to tell the user
// the rest existed. Paging reintroduces exactly that hazard, so the protection
// is REPLACED rather than removed: `next_cursor` and `total` come back with the
// page, and Invariant 5 requires the caller to render what it has not loaded
// instead of leaving it to be inferred from a button. Dropping the walk without
// that disclosure would be a regression wearing a performance costume.
export async function listTenantModelEntries(
  token: string,
  startingAfter: string | null = null,
): Promise<TenantModelEntryPageResult> {
  const params = new URLSearchParams({ limit: String(REGISTRY_PAGE_LIMIT) });
  if (startingAfter !== null) params.set("starting_after", startingAfter);

  const body = await request<TenantModelEntryPage>(
    `/v1/tenants/me/models?${params.toString()}`,
    { method: "GET" },
    token,
  );

  // Both platform-default fields describe the tenant rather than the page, and
  // every page recomputes them, so any page's answer is authoritative.
  return {
    models: body.models,
    platform_default_available: body.platform_default_available,
    platform_default: body.platform_default,
    next_cursor: body.next_cursor,
    total: body.total,
  };
}

export async function createTenantModelEntry(
  body: { model_id: string; secret_ref: string },
  token: string,
): Promise<TenantModelEntryWriteResult> {
  return request<TenantModelEntryWriteResult>(
    "/v1/tenants/me/models",
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

export async function updateTenantModelEntry(
  id: string,
  body: { model_id: string },
  token: string,
): Promise<TenantModelEntryWriteResult> {
  return request<TenantModelEntryWriteResult>(
    `/v1/tenants/me/models/${encodeURIComponent(id)}`,
    { method: "PATCH", body: JSON.stringify(body) },
    token,
  );
}

export async function deleteTenantModelEntry(id: string, token: string): Promise<void> {
  return request<void>(
    `/v1/tenants/me/models/${encodeURIComponent(id)}`,
    { method: "DELETE" },
    token,
  );
}
