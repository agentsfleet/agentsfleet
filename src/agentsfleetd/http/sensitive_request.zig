//! Dispatcher-owned cleanup for routes whose request bodies carry secrets.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const metrics = @import("../observability/metrics_sensitive_memory.zig");

pub fn eraseAfterDispatch(req: *httpz.Request, route: router.Route) void {
    if (!hasSensitiveBody(route, req.method)) return;
    if (req.body()) |body| {
        common.secureZeroRequestBody(body);
        metrics.recordRequestErased(body.len);
    }
}

fn hasSensitiveBody(route: router.Route, method: httpz.Method) bool {
    return switch (route) {
        .workspace_secrets => method == .POST,
        .workspace_secret => method == .PATCH,
        .runner_credentials_mint => method == .POST,
        else => false,
    };
}
