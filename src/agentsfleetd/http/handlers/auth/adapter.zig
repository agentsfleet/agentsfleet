//! auth_adapter — bridge between handler.Context and the auth middleware layer.
//!
//! The dispatcher calls `buildAuthCtx` to construct an `AuthCtx` from the
//! request-scoped state (arena allocator, request ID, response) before running
//! the middleware chain. `common.errorResponse` is injected as the write_error
//! callback so middlewares can write RFC 7807 error bodies without importing
//! `src/errors/` themselves.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const auth_mw = @import("../../../auth/middleware/mod.zig");

/// Construct an `AuthCtx` wired to the host's error-writer.
///
/// The returned `AuthCtx` borrows `res` — it must not outlive the response.
/// `alloc` should be an arena tied to the current request.
pub fn buildAuthCtx(
    res: *httpz.Response,
    alloc: std.mem.Allocator,
    req_id: []const u8,
) auth_mw.AuthCtx {
    return .{
        .alloc = alloc,
        .res = res,
        .req_id = req_id,
        .write_error = common.errorResponse,
    };
}
