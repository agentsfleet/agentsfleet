//! Fleet memory invokes split from route_table_invoke.zig for the line cap.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");
const memory = @import("handlers/memory/handler.zig");

const Hx = hx_mod.Hx;

pub fn invokeFleetMemoriesCollection(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_fleet_memories;
    switch (req.method) {
        .GET => memory.innerListMemories(hx.*, req, r.workspace_id, r.fleet_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeFleetMemoryItem(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_fleet_memory_item;
    switch (req.method) {
        .DELETE => memory.innerDeleteMemory(hx.*, r.workspace_id, r.fleet_id, r.memory_key),
        else => common.respondMethodNotAllowed(hx.res),
    }
}
