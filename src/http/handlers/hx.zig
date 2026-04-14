// M11_002: Request-scoped handler context.
//
// M18_002 Batch D: `authenticated()` and `authenticatedWithParam()` wrappers
// removed. Auth lives in the middleware chain (route_table.zig + auth/middleware/).
// Hx is now constructed by the dispatcher after the chain runs; handlers receive
// a populated Hx directly.
//
// Usage (post-M18_002):
//   fn innerCreateZombie(hx: Hx, req: *httpz.Request) void { ... }
//   // Route registered in route_table.zig with bearer policy.
//   // Dispatcher builds Hx from AuthCtx and calls invokeXxx.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");

pub const Hx = struct {
    alloc: std.mem.Allocator,
    /// Populated by bearer/admin middleware for authenticated routes.
    /// Zero-value (.mode = .api_key) for none-policy routes — those
    /// handlers must not access this field (Batch E will make it optional).
    principal: common.AuthPrincipal,
    req_id: []const u8,
    ctx: *common.Context,
    res: *httpz.Response,

    /// Write a successful JSON response.
    pub fn ok(self: Hx, status: std.http.Status, body: anytype) void {
        common.writeJson(self.res, status, body);
    }

    /// Write an RFC 7807 error response. HTTP status is owned by the error code table.
    pub fn fail(self: Hx, code: []const u8, detail: []const u8) void {
        common.errorResponse(self.res, code, detail, self.req_id);
    }
};
