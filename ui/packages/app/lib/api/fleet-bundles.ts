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
// The server validates + content-addresses the snapshot; it does NOT fetch
// GitHub. The caller assembles the Markdown (see lib/github/fetch-bundle.ts)
// and posts it here. `source_ref` is recorded as provenance only.

// First-party template catalog. Metadata only (id/name/description + declared
// requirement names) — the SKILL.md/TRIGGER.md content lives in
// github.com/agentsfleet/skills and is fetched app-side before import.
export async function listFleetTemplates(
  token: string,
): Promise<FleetTemplateListResponse> {
  return request<FleetTemplateListResponse>(
    "/v1/fleets/bundles",
    { method: "GET" },
    token,
  );
}

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

// Re-read a stored snapshot's parsed detail (requirements, support files,
// content hash). Used when resuming from a `?bundle_id=` deep link.
export async function getBundleSnapshot(
  workspaceId: string,
  bundleId: string,
  token: string,
): Promise<BundleSnapshot> {
  return request<BundleSnapshot>(
    `/v1/workspaces/${workspaceId}/fleets/bundles/snapshots/${bundleId}`,
    { method: "GET" },
    token,
  );
}
