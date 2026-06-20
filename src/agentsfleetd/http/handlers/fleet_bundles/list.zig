const httpz = @import("httpz");

const fleet_bundle = @import("../../../fleet_bundle/mod.zig");
const hx_mod = @import("../hx.zig");

pub fn innerList(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    hx.ok(.ok, .{ .items = fleet_bundle.template_catalog.all() });
}
