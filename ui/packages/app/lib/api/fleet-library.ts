import { cache } from "react";
import { request } from "./client";
import type {
  FleetLibraryGalleryResponse,
  OnboardedLibraryEntry,
  OnboardedPlatformLibraryEntry,
  OnboardLibraryEntryRequest,
  PlatformCatalogEntry,
  PlatformCatalogPatch,
  PlatformCatalogResponse,
} from "../types";

const workspaceFleetLibrariesPath = (workspaceId: string) =>
  `/v1/workspaces/${workspaceId}/fleet-libraries`;

// The platform catalog has no workspace segment — it is a single tier shared by
// every tenant, gated on `platform-library:write` rather than workspace
// ownership (src/agentsfleetd/http/route_scopes.zig). Every method needs that same
// scope: there is no read rung.
const PLATFORM_FLEET_LIBRARIES_PATH = "/v1/admin/fleet-libraries";
const platformEntryPath = (id: string) =>
  `${PLATFORM_FLEET_LIBRARIES_PATH}/${encodeURIComponent(id)}`;

// Fleet library gallery client. Mirrors src/agentsfleetd/http/routes.zig:
//   GET /v1/workspaces/{ws}/fleet-libraries  (platform ∪ own-tenant entries)
//
// The gallery returns the union of the platform catalog and the caller-
// workspace's own tenant entries — and nothing from another workspace. Each
// entry carries `visibility`, so the install flow keys the create body off the
// chosen tier (platform_library_id vs tenant_library_id). Metadata only — the
// canonical bundle bytes live in R2, never in the response.
export async function listWorkspaceFleetLibrary(
  workspaceId: string,
  token: string,
): Promise<FleetLibraryGalleryResponse> {
  return request<FleetLibraryGalleryResponse>(
    workspaceFleetLibrariesPath(workspaceId),
    { method: "GET" },
    token,
  );
}

export async function onboardWorkspaceFleetLibrary(
  workspaceId: string,
  body: OnboardLibraryEntryRequest,
  token: string,
): Promise<OnboardedLibraryEntry> {
  return request<OnboardedLibraryEntry>(
    workspaceFleetLibrariesPath(workspaceId),
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Onboard an entry into the PLATFORM catalog — the operator-tier counterpart of
// `onboardWorkspaceFleetLibrary` above. The server fetches the repository,
// validates the bundle, writes the canonical tar to object storage, and only
// then upserts the catalog row, taking the row id from the bundle's SKILL.md
// frontmatter name. The onboarded row is stored `public`, which is what puts it
// in every workspace's gallery beside the migration-seeded rows.
//
// The row is stored `draft` (M128): adding a fleet never publishes it. It reaches
// no tenant until `publishPlatformFleetLibraryEntry` says so.
export async function onboardPlatformFleetLibrary(
  body: OnboardLibraryEntryRequest,
  token: string,
): Promise<OnboardedPlatformLibraryEntry> {
  return request<OnboardedPlatformLibraryEntry>(
    PLATFORM_FLEET_LIBRARIES_PATH,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Per-request deduped gallery read. The gallery is rarely-changing metadata;
// React's cache() collapses repeat reads within one RSC render (the dashboard
// gallery and /fleets/new both list library entries) to a single round-trip.
// Server-only — cache() is a React Server Component primitive.
export const listWorkspaceFleetLibraryCached = cache(listWorkspaceFleetLibrary);

// ── The platform catalog (M128) ──────────────────────────────────────────────
//
// The operator's view of core.fleet_library. Unlike the workspace gallery it
// hides nothing: drafts and rows whose bundle was never fetched are exactly what
// the page exists to show. Metadata only — the server never returns bundle
// markdown or an object-store key.

export async function listPlatformFleetLibrary(token: string): Promise<PlatformCatalogResponse> {
  return request<PlatformCatalogResponse>(PLATFORM_FLEET_LIBRARIES_PATH, { method: "GET" }, token);
}

// Curate the two fields no bundle can supply, and/or publish/unpublish. A bundle
// refetch never overwrites what this writes.
export async function patchPlatformFleetLibraryEntry(
  id: string,
  body: PlatformCatalogPatch,
  ifMatch: string,
  token: string,
): Promise<PlatformCatalogEntry> {
  return request<PlatformCatalogEntry>(
    platformEntryPath(id),
    { method: "PATCH", headers: { "If-Match": ifMatch }, body: JSON.stringify(body) },
    token,
  );
}

// Remove an entry. The server refuses while it is published (UZ-CATALOG-003) —
// a live fleet is never taken from the tenants who can install it.
export async function deletePlatformFleetLibraryEntry(id: string, token: string): Promise<void> {
  await request<void>(platformEntryPath(id), { method: "DELETE" }, token);
}
