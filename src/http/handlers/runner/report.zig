//! POST /v1/runners/me/reports — report a terminal execution.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); the service loads the
//! lease, reproduces the direct worker's finalize writes (terminal status +
//! telemetry actuals + session checkpoint), XACKs, and marks the lease
//! reported. Idempotency/fencing verification are a follow-up — the runner
//! token owns the identity; no `runner_id` rides in the body.

const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const service_report = @import("../../../runner/service_report.zig");

const Hx = hx_mod.Hx;

pub fn innerRunnerReport(hx: Hx, req: *httpz.Request) void {
    service_report.report(hx, req);
}
