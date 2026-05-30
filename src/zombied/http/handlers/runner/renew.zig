//! POST /v1/runners/me/leases/{lease_id}/renew — extend a live lease deadline.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); `lease_id` is the path
//! param. The service credit-gates, then atomically extends both the lease row
//! and its affinity slot under the live fence, and answers with the new deadline
//! or a terminal code (010 max-runtime / 011 lost / 012 no-credits).

const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const service_renew = @import("../../../fleet/service_renew.zig");

const Hx = hx_mod.Hx;

pub fn innerRunnerRenew(hx: Hx, req: *httpz.Request, lease_id: []const u8) void {
    service_renew.renew(hx, req, lease_id);
}
