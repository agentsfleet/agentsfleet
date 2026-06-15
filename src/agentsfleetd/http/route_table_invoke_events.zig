//! M42 events invokes split out of route_table_invoke.zig to keep that
//! file ≤ 350 lines per RULE FLL.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const agent_events = @import("handlers/agents/events.zig");
const agent_events_stream_h = @import("handlers/agents/events_stream.zig");
const workspace_events_h = @import("handlers/workspaces/events.zig");

const Hx = hx_mod.Hx;

pub fn invokeAgentEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_agent_events;
    agent_events.innerListEvents(hx.*, req, r.workspace_id, r.agent_id);
}

pub fn invokeAgentEventsStream(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_agent_events_stream;
    agent_events_stream_h.innerEventsStream(hx.*, req, r.workspace_id, r.agent_id);
}

pub fn invokeWorkspaceEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    workspace_events_h.innerListWorkspaceEvents(hx.*, req, route.workspace_events);
}
