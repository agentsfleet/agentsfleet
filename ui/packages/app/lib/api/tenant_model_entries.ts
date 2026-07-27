import { request } from "./client";
import type {
  TenantModelEntry,
  TenantModelEntryList,
  TenantModelEntryWriteResult,
} from "../types";

// GET/POST/PATCH/DELETE /v1/tenants/me/models — see
// src/agentsfleetd/http/handlers/tenant_model_entries.zig for the wire
// contract. `api_key` never appears in any response; each entry carries only
// `has_key` plus the metadata joined from its referenced secret.

// Rows per request, and the ceiling on how many requests one walk will make.
// The endpoint pages at 50 by default and rejects a `limit` above 100
// (`UZ-LIBRARY-003`), so asking for the maximum halves the round-trips a
// large registry costs.
const REGISTRY_PAGE_LIMIT = 100;
const REGISTRY_MAX_PAGES = 50;

/** One wire page: `models` is that page alone, `next_cursor` null on the last. */
type TenantModelEntryPage = TenantModelEntryList & {
  total: number | null;
  next_cursor: string | null;
};

// The Models page renders the entire registry, so this follows `next_cursor`
// to exhaustion instead of returning page one. Reading a single page would
// drop every entry past the server's page size *silently* — the rows simply
// would not render, with nothing to tell the user they still exist.
export async function listTenantModelEntries(token: string): Promise<TenantModelEntryList> {
  const models: TenantModelEntry[] = [];
  let cursor: string | null = null;

  for (let page = 0; page < REGISTRY_MAX_PAGES; page += 1) {
    const params = new URLSearchParams({ limit: String(REGISTRY_PAGE_LIMIT) });
    if (cursor !== null) params.set("starting_after", cursor);

    const body = await request<TenantModelEntryPage>(
      `/v1/tenants/me/models?${params.toString()}`,
      { method: "GET" },
      token,
    );
    models.push(...body.models);

    if (!body.next_cursor) {
      // Both platform-default fields describe the tenant rather than the page,
      // and every page recomputes them, so the last page's answer is as
      // authoritative as the first's.
      return {
        models,
        platform_default_available: body.platform_default_available,
        platform_default: body.platform_default,
      };
    }
    cursor = body.next_cursor;
  }

  // Throw rather than return the rows collected so far. A walk reaches this
  // bound only if the server stopped advancing its cursor, and then `models`
  // holds one page repeated — rendering that would show duplicate rows as
  // though they were distinct registry entries, which is worse than an error.
  throw new Error(
    `tenant model registry did not terminate within ${REGISTRY_MAX_PAGES} pages`,
  );
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
