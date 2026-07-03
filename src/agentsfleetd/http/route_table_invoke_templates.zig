//! Invoke wrappers for the two template onboarding routes (M103). Each enforces
//! POST, extracts path params from `route`, and calls the inner handler (the
//! capability scope is already enforced by requireScope middleware).

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");
const templates = @import("handlers/templates/api.zig");

const Hx = hx_mod.Hx;

pub fn invokePlatformTemplateOnboard(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (!common.requireMethod(hx.res, req.method, .POST)) return;
    templates.innerPlatformOnboard(hx.*, req);
}

/// GET lists the workspace gallery (platform ∪ this workspace's tenant
/// templates); POST onboards a tenant template. Both carry workspace_id.
pub fn invokeWorkspaceFleetTemplates(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_fleet_templates;
    switch (req.method) {
        .GET => templates.innerGallery(hx.*, workspace_id),
        .POST => templates.innerTenantOnboard(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}
