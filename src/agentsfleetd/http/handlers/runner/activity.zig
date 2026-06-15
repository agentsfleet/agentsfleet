//! POST /v1/runners/me/leases/{lease_id}/activity — forward live-tail frames.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); `lease_id` is the only
//! runner-verb path param. The service resolves the lease's zombie + event
//! (scoped to the runner) and `PUBLISH`es each frame to `agent:{id}:activity`
//! for the SSE live tail. Best-effort: a dropped frame is cosmetic, the durable
//! record is `report`.

const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const service_activity = @import("../../../fleet/service_activity.zig");

const Hx = hx_mod.Hx;

pub fn innerRunnerActivity(hx: Hx, req: *httpz.Request, lease_id: []const u8) void {
    service_activity.activity(hx, req, lease_id);
}
