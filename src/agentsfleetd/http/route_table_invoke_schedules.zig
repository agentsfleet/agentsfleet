//! Schedule invokes split out of route_table_invoke.zig for the line cap.

const httpz = @import("httpz");
const router = @import("router.zig");
const hx_mod = @import("handlers/hx.zig");
const schedules = @import("handlers/schedules/api.zig");

const Hx = hx_mod.Hx;

pub fn invokeScheduleCollection(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_fleet_schedules;
    schedules.innerScheduleCollection(hx.*, req, r.workspace_id, r.fleet_id);
}

pub fn invokeScheduleItem(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_fleet_schedule;
    schedules.innerScheduleItem(hx.*, req, r.workspace_id, r.fleet_id, r.schedule_id);
}

pub fn invokeScheduleSync(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_fleet_schedule_sync;
    schedules.innerScheduleSync(hx.*, req, r.workspace_id, r.fleet_id, r.schedule_id);
}
