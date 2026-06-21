const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");
const bundles = @import("handlers/fleet_bundles/api.zig");

const Hx = hx_mod.Hx;

pub fn invokeFleetBundles(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    bundles.innerList(hx.*, req);
}

pub fn invokeFleetBundleImports(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    bundles.innerImport(hx.*, req, route.workspace_fleet_bundles);
}

pub fn invokeFleetBundleGet(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_fleet_bundle;
    bundles.innerGet(hx.*, req, r.workspace_id, r.bundle_id);
}
