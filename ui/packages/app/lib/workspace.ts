import { cache } from "react";
import { listTenantWorkspaces } from "./api/workspaces";

/**
 * The switcher, route guard and current page share the full workspace list
 * during this render. The login redirect reads only until it finds the default
 * workspace. Every daemon call authorizes the workspace independently.
 */
export const listTenantWorkspacesCached = cache(listTenantWorkspaces);
