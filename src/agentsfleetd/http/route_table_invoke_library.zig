//! Invoke wrappers for the two Fleet library onboarding routes (M103). Each
//! enforces POST, extracts path params from `route`, and calls the inner handler
//! (the capability scope is already enforced by requireScope middleware).

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");
const library = @import("handlers/library/api.zig");

const Hx = hx_mod.Hx;

pub fn invokePlatformLibraryOnboard(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (!common.requireMethod(hx.res, req.method, .POST)) return;
    library.innerPlatformOnboard(hx.*, req);
}

/// GET lists the workspace gallery (platform ∪ this workspace's tenant
/// entries); POST onboards a tenant entry. Both carry workspace_id.
pub fn invokeWorkspaceFleetLibrary(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_fleet_library;
    switch (req.method) {
        .GET => library.innerGallery(hx.*, workspace_id),
        .POST => library.innerTenantOnboard(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}
