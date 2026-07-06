/**
 * Workspace-URL navigation helpers (M118).
 *
 * The dashboard workspace is now an explicit URL segment (`/w/<id>/…`), so e2e
 * specs navigate to `/w/<default-workspace>/<subpath>` rather than a bare
 * root-relative page. `getDefaultWorkspaceId` resolves the same "first owned"
 * workspace the app's entry redirect lands on, so a deep-link here matches what
 * a user reaches via `/`.
 */
import type { Page } from "@playwright/test";
import { getDefaultWorkspaceId } from "./seed";
import type { ClientHandle } from "./api-client";

// A URL matcher for a workspace-scoped page: `/w/<any-id>/<subpath>` with an
// optional trailing query. Pass "" to match the workspace home (`/w/<id>`).
export function workspaceUrlPattern(subpath = ""): RegExp {
  const clean = subpath.replace(/^\/+/, "");
  const escaped = clean.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const tail = escaped ? `/${escaped}` : "";
  return new RegExp(`/w/[^/]+${tail}(\\?|$)`);
}

// Resolve the fixture user's default workspace and navigate to a workspace-scoped
// page under it. Returns the workspace id so a spec can build deeper URLs.
export async function gotoWorkspace(
  page: Page,
  handle: ClientHandle,
  subpath = "",
): Promise<string> {
  const workspaceId = await getDefaultWorkspaceId(handle);
  const clean = subpath.replace(/^\/+/, "");
  await page.goto(clean ? `/w/${workspaceId}/${clean}` : `/w/${workspaceId}`);
  return workspaceId;
}

// Build a workspace-scoped path for a known workspace id (no navigation).
export function workspaceHref(workspaceId: string, subpath = ""): string {
  const clean = subpath.replace(/^\/+/, "");
  return clean ? `/w/${workspaceId}/${clean}` : `/w/${workspaceId}`;
}
