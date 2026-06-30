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
const errors = @import("errors.zig");

pub const AuthPrincipal = principal_mod.AuthPrincipal;

/// Error-writing callback supplied by the host. Abstracts RFC 7807 body
/// assembly so the middleware layer never imports `src/errors/`. `detail` is
/// borrowed for the call only — callers may pass a stack slice, so the host
/// must serialize/copy it before returning, never retain the slice.
pub const WriteErrorFn = *const fn (
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    req_id: []const u8,
) void;

pub const AuthCtx = struct {
    const Self = @This();

    alloc: std.mem.Allocator,
    res: *httpz.Response,
    req_id: []const u8,
    principal: ?AuthPrincipal = null,
    write_error: WriteErrorFn,

    // Per-request slot for webhook routes — the dispatcher populates this from
    // the matched route's fleet_id before calling chain.run. The webhook_sig
    // and svix middlewares read it; all others ignore it.
    webhook_fleet_id: ?[]const u8 = null,

    // Per-request capability requirement — the dispatcher resolves it from the
    // matched route + HTTP method (`route_scopes.requiredScopes`) before running
    // the chain. `requireScope` reads it as an any-of set; empty means the route
    // requires authentication only (no capability scope). Set by the host so the
    // auth layer never imports the HTTP route table (portability boundary).
    required_scopes: []const principal_mod.Scope = &.{},

    /// Write a problem+json error response via the host-supplied writer.
    /// The HTTP status comes from the host's error table (middleware does
    /// not know it).
    pub fn fail(self: *Self, code: []const u8, detail: []const u8) void {
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

    ctx.fail(errors.ERR_FORBIDDEN, "Invalid or missing token");

    try testing.expectEqual(@as(usize, 1), test_write_count);
    try testing.expectEqualStrings(errors.ERR_FORBIDDEN, test_last_code);
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
