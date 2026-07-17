// Dashboard workspace-URL helpers. The workspace is an explicit route segment
// (`/w/<workspaceId>/…`), mirroring the backend/CLI `/v1/workspaces/{ws}` path
// and Supabase Studio's `/project/[ref]`. These are pure, client-safe string
// helpers — the single source of truth for the `/w` prefix so no page or nav
// item hand-writes the literal (UFS: one named constant, no duplicated paths).

/** Route prefix for the workspace-scoped dashboard segment (`app/(dashboard)/w/[workspaceId]`). */
export const WORKSPACE_ROUTE_PREFIX = "/w";
export const DEFAULT_WORKSPACE_SUBPATH = "fleets";

// Matches a leading `/w/<id>` and captures the id. Built from the prefix so the
// literal lives in exactly one place.
const WORKSPACE_PATH_RE = new RegExp(`^${WORKSPACE_ROUTE_PREFIX}/([^/]+)`);

/**
 * Builds a workspace-scoped path: `workspacePath("ws_1", "fleets")` →
 * `/w/ws_1/fleets`; `workspacePath("ws_1")` → `/w/ws_1`. A leading slash on
 * `subpath` is tolerated so callers can pass either `"fleets"` or `"/fleets"`.
 */
export function workspacePath(workspaceId: string, subpath = ""): string {
  const clean = subpath.replace(/^\/+/, "");
  const base = `${WORKSPACE_ROUTE_PREFIX}/${workspaceId}`;
  return clean ? `${base}/${clean}` : base;
}

/**
 * Extracts the workspace id from a pathname when it sits under the `/w/<id>`
 * segment, else `null` (tenant/platform routes like `/settings/api-keys` carry
 * no workspace). Used by the client Shell/Switcher to derive the active
 * workspace from the route rather than a cookie.
 */
export function workspaceIdFromPath(pathname: string): string | null {
  return WORKSPACE_PATH_RE.exec(pathname)?.[1] ?? null;
}

/**
 * Returns the portion of a pathname *after* the `/w/<id>` prefix (no leading
 * slash), or `""` when the path is the workspace root or not workspace-scoped.
 * Lets the switcher preserve the current sub-page across a workspace change:
 * `/w/a/fleets` + new id `b` → `workspacePath("b", subpath)` = `/w/b/fleets`.
 */
export function workspaceSubpath(pathname: string): string {
  const id = workspaceIdFromPath(pathname);
  if (!id) return "";
  const rest = pathname.slice(`${WORKSPACE_ROUTE_PREFIX}/${id}`.length);
  return rest.replace(/^\/+/, "");
}

// Sections whose deeper path segment is a resource id owned by the *current*
// workspace — the target workspace won't own that same id, so a switch that kept
// it would land on a guaranteed `notFound()`.
const RESOURCE_DETAIL_SECTIONS = new Set(["fleets", "approvals"]);

/**
 * Maps a workspace sub-path to the sub-path a workspace *switch* should land on.
 * An empty path lands on the fleet wall instead of the redirect-only root.
 * A resource-detail path (`fleets/<id>`, `approvals/<gateId>`) collapses to its
 * section root (`fleets`, `approvals`) — the target workspace doesn't own that
 * resource. Generic pages (`fleets/new`, `settings/models`, `integrations`, …)
 * are preserved verbatim so switching keeps you in the same section.
 */
export function workspaceSwitchSubpath(subpath: string): string {
  const [section, detail] = subpath.split("/").filter(Boolean);
  if (!section) return DEFAULT_WORKSPACE_SUBPATH;
  if (section && detail && detail !== "new" && RESOURCE_DETAIL_SECTIONS.has(section)) {
    return section;
  }
  return subpath;
}
