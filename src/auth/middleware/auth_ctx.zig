//! Minimal per-request context owned by the auth middleware layer.
//!
//! Middlewares receive `*AuthCtx` — NOT `*Hx`. Keeping the shape small and
//! HTTP-layer-agnostic preserves the §1.2 portability contract: `src/auth/`
//! never imports from `src/http/handlers/`.
//!
//! The host (dispatcher) constructs an `AuthCtx`, injects its error-writer
//! callback, and hands `&ctx` to the chain runner. Handler types like `Hx`
//! can embed an `AuthCtx` and expose their own conveniences on top.

const std = @import("std");
const httpz = @import("httpz");
const principal_mod = @import("../principal.zig");

pub const AuthPrincipal = principal_mod.AuthPrincipal;

/// Error-writing callback supplied by the host. Abstracts RFC 7807 body
/// assembly so the middleware layer never imports `src/errors/`.
pub const WriteErrorFn = *const fn (
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    req_id: []const u8,
) void;

pub const AuthCtx = struct {
    alloc: std.mem.Allocator,
    res: *httpz.Response,
    req_id: []const u8,
    principal: ?AuthPrincipal = null,
    write_error: WriteErrorFn,

    /// Write a problem+json error response via the host-supplied writer.
    /// The HTTP status comes from the host's error table (middleware does
    /// not know it).
    pub fn fail(self: *AuthCtx, code: []const u8, detail: []const u8) void {
        self.write_error(self.res, code, detail, self.req_id);
    }
};

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

var test_last_code: []const u8 = "";
var test_last_detail: []const u8 = "";
var test_last_req_id: []const u8 = "";
var test_write_count: usize = 0;

fn testWriteError(
    _: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    req_id: []const u8,
) void {
    test_last_code = code;
    test_last_detail = detail;
    test_last_req_id = req_id;
    test_write_count += 1;
}

test "AuthCtx.fail forwards code/detail/req_id to host writer" {
    test_last_code = "";
    test_last_detail = "";
    test_last_req_id = "";
    test_write_count = 0;

    var res: httpz.Response = undefined;
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = &res,
        .req_id = "req_abcdef012345",
        .write_error = testWriteError,
    };

    ctx.fail("UZ-AUTH-001", "Invalid or missing token");

    try testing.expectEqual(@as(usize, 1), test_write_count);
    try testing.expectEqualStrings("UZ-AUTH-001", test_last_code);
    try testing.expectEqualStrings("Invalid or missing token", test_last_detail);
    try testing.expectEqualStrings("req_abcdef012345", test_last_req_id);
}

test "AuthCtx defaults principal to null until a middleware populates it" {
    var res: httpz.Response = undefined;
    const ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = &res,
        .req_id = "req_x",
        .write_error = testWriteError,
    };
    try testing.expect(ctx.principal == null);
}
