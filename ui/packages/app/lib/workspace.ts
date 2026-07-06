import { cache } from "react";
import { listTenantWorkspaces } from "./api/workspaces";

/**
 * Per-request deduped wrapper around `listTenantWorkspaces`. Wrapping with
 * React's `cache()` makes every caller in a single RSC render share one
 * round-trip. Without this, a dashboard load can fire 4+ redundant
 * GET /v1/tenants/me/workspaces calls (root layout + `[workspaceId]` guard +
 * switcher + entry redirect).
 *
 * This is the ONLY workspace helper the dashboard needs: the active workspace
 * is now an explicit URL segment (`/w/<id>/…`, see `lib/workspace-routes.ts`),
 * not a cookie/claim the dashboard resolves. The `[workspaceId]` layout
 * validates the route id against this list (a UX guard); the security boundary
 * stays `ownsWithinTenant`, server-side, on every backend call.
 */
export const listTenantWorkspacesCached = cache(listTenantWorkspaces);
