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
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    templates.innerPlatformOnboard(hx.*, req);
}

pub fn invokeTenantTemplateOnboard(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    templates.innerTenantOnboard(hx.*, req, route.workspace_fleet_templates);
}
