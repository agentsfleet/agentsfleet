//! M42 events invokes split out of route_table_invoke.zig to keep that
//! file ≤ 350 lines per RULE FLL.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const fleet_events = @import("handlers/fleets/events.zig");
const fleet_events_stream_h = @import("handlers/fleets/events_stream.zig");
const workspace_events_h = @import("handlers/workspaces/events.zig");
const workspace_events_stream_h = @import("handlers/workspaces/events_stream.zig");

const Hx = hx_mod.Hx;

pub fn invokeFleetEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    const r = route.workspace_fleet_events;
    fleet_events.innerListEvents(hx.*, req, r.workspace_id, r.fleet_id);
}

pub fn invokeFleetEventsStream(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    const r = route.workspace_fleet_events_stream;
    fleet_events_stream_h.innerEventsStream(hx.*, req, r.workspace_id, r.fleet_id);
}

pub fn invokeWorkspaceEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    workspace_events_h.innerListWorkspaceEvents(hx.*, req, route.workspace_events);
}

pub fn invokeWorkspaceEventsStream(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (!common.requireMethod(hx.res, req.method, .GET)) return;
    workspace_events_stream_h.innerWorkspaceEventsStream(hx.*, req, route.workspace_events_stream);
}
