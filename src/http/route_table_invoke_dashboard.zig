//! Dashboard invokes split out of route_table_invoke.zig to keep that file
//! ≤ 350 lines per RULE FLL.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const zombie_lifecycle = @import("handlers/zombies/lifecycle.zig");

const Hx = hx_mod.Hx;

pub fn invokeDeleteCurrentRun(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_zombie_current_run;
    zombie_lifecycle.innerDeleteCurrentRun(hx.*, req, r.workspace_id, r.zombie_id);
}
