//! Clerk signup webhook handler (stub — real implementation pending).
//!
//! Keeps the build green now that route_table_invoke wires
//! `invokeClerkWebhook -> innerClerkWebhook`. Returns 500 so anyone who
//! reaches this route before the real handler lands gets a clear signal
//! instead of a crash or a silently-accepted webhook.

const httpz = @import("httpz");
const hx_mod = @import("hx.zig");
const common = @import("common.zig");

pub fn innerClerkWebhook(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    common.internalOperationError(hx.res, "clerk webhook handler not yet implemented", hx.req_id);
}
