const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");
const bundles = @import("handlers/fleet_bundles/api.zig");

const Hx = hx_mod.Hx;

/// GET /v1/fleets/bundles — the platform Fleet template catalog (the gallery
/// shop-window). The workspace gallery union lives at
/// GET /v1/workspaces/{ws}/fleet-templates.
pub fn invokeFleetBundles(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    bundles.innerList(hx.*, req);
}
