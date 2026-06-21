import { cache } from "react";
import { request } from "./client";
import type {
  BundleSnapshot,
  FleetTemplateListResponse,
  ImportBundleRequest,
} from "../types";

// Fleet Bundle API client — the source/import layer above Fleet creation.
// Routes mirror src/agentsfleetd/http/routes.zig:
//   GET  /v1/fleets/bundles                                  (template catalog)
//   POST /v1/workspaces/{ws}/fleets/bundles/snapshots        (import + validate)
//   GET  /v1/workspaces/{ws}/fleets/bundles/snapshots/{id}   (parsed detail)
//
// The server fetches (for `github`/`template`), validates, and content-
// addresses the snapshot; the dashboard posts `{ source_kind, source_ref }`
// and the server-fetched bundle is authoritative (no app-side GitHub fetch).

// First-party template catalog. Metadata only (id/name/description + declared
// requirement names) — the SKILL.md/TRIGGER.md content is fetched server-side
// at import time from the template's pinned source.
export async function listFleetTemplates(
  token: string,
): Promise<FleetTemplateListResponse> {
  return request<FleetTemplateListResponse>(
    "/v1/fleets/bundles",
    { method: "GET" },
    token,
  );
}

// Per-request deduped catalog read. The catalog is first-party, rarely-changing
// metadata; React's cache() collapses repeat reads within one RSC render (the
// dashboard gallery and /fleets/new both list templates) to a single
// round-trip. Server-only — cache() is a React Server Component primitive.
export const listFleetTemplatesCached = cache(listFleetTemplates);

// Import (validate + snapshot) a bundle the caller already assembled. Returns
// the immutable `bundle_id` plus parsed `requirements` for the install preview.
// Throws ApiError on validation failure (missing_skill, unsafe_path, etc.).
export async function importBundleSnapshot(
  workspaceId: string,
  body: ImportBundleRequest,
  token: string,
): Promise<BundleSnapshot> {
  return request<BundleSnapshot>(
    `/v1/workspaces/${workspaceId}/fleets/bundles/snapshots`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}
