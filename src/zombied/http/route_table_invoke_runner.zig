//! Runner + fleet operator-plane invoke functions — split from
//! route_table_invoke.zig to keep it <= 350 lines (RULE FLL). Re-exported there
//! as `invokeXxx`, so route_table.zig dispatch references are unchanged. Each
//! checks the HTTP method (405 if wrong) and delegates to the inner handler.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const runner_register = @import("handlers/runner/register.zig");
const fleet_runners_list = @import("handlers/fleet/runners_list.zig");
const fleet_runner_patch = @import("handlers/fleet/runner_patch.zig");
const runner_self = @import("handlers/runner/self.zig");
const runner_heartbeat = @import("handlers/runner/heartbeat.zig");
const runner_lease = @import("handlers/runner/lease.zig");
const runner_report = @import("handlers/runner/report.zig");
const runner_activity = @import("handlers/runner/activity.zig");
const runner_renew = @import("handlers/runner/renew.zig");
const runner_memory = @import("handlers/runner/memory.zig");

const Hx = hx_mod.Hx;

// ── Runner control plane ──────────────────────────────────────────────────
// POST-only; wrong methods get 405. Each invoke delegates to the real handler
// (register mints + persists; heartbeat/lease/report drive the control plane).

pub fn invokeRegisterRunner(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_register.innerRegisterRunner(hx.*, req);
}

pub fn invokeFleetRunnersList(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    fleet_runners_list.innerListFleetRunners(hx.*, req);
}

pub fn invokeFleetRunnerPatch(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .PATCH) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    fleet_runner_patch.innerPatchFleetRunner(hx.*, req, route.fleet_runner_patch);
}

pub fn invokeRunnerSelf(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_self.innerRunnerSelf(hx.*, req);
}

pub fn invokeRunnerHeartbeat(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_heartbeat.innerRunnerHeartbeat(hx.*, req);
}

pub fn invokeRunnerLease(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_lease.innerRunnerLease(hx.*, req);
}

pub fn invokeRunnerReport(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_report.innerRunnerReport(hx.*, req);
}

pub fn invokeRunnerActivity(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_activity.innerRunnerActivity(hx.*, req, route.runner_activity);
}

pub fn invokeRunnerRenew(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_renew.innerRunnerRenew(hx.*, req, route.runner_renew);
}

pub fn invokeRunnerMemoryHydrate(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_memory.innerRunnerMemoryHydrate(hx.*, route.runner_memory_hydrate);
}

pub fn invokeRunnerMemoryCapture(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    runner_memory.innerRunnerMemoryCapture(hx.*, req, route.runner_memory_capture);
}
