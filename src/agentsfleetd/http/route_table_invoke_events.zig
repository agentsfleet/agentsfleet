//! M42 events invokes split out of route_table_invoke.zig to keep that
//! file ≤ 350 lines per RULE FLL.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const zombie_events = @import("handlers/zombies/events.zig");
const zombie_events_stream_h = @import("handlers/zombies/events_stream.zig");
const workspace_events_h = @import("handlers/workspaces/events.zig");

const Hx = hx_mod.Hx;

pub fn invokeZombieEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_zombie_events;
    zombie_events.innerListEvents(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeZombieEventsStream(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_zombie_events_stream;
    zombie_events_stream_h.innerEventsStream(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeWorkspaceEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    workspace_events_h.innerListWorkspaceEvents(hx.*, req, route.workspace_events);
}
