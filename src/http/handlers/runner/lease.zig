//! POST /v1/runners/me/leases — long-poll for the next event.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); the service claims the
//! runner's one assigned zombie, bills the event, persists a
//! `fleet.runner_leases` row, and returns 200 `{ lease | null, retry_after_ms }`
//! — never a 204.

const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const service = @import("../../../runner/service.zig");

const Hx = hx_mod.Hx;

pub fn innerRunnerLease(hx: Hx, req: *httpz.Request) void {
    _ = req; // S0 lease request body is empty; the long-poll is server-side.
    service.leaseNext(hx);
}
