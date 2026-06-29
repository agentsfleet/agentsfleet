//! `require_scope` middleware — the single capability gate.
//!
//! Replaces `require_role.zig` and `platform_admin.zig`. Composes *after* an
//! auth middleware that populated `ctx.principal`. The route's required scopes
//! (`ctx.required_scopes`, an any-of set the host resolved from the route table
//! + HTTP method) are checked against the principal's hierarchy-expanded
//! `scopes`. Allows iff the principal holds *any* required scope; otherwise
//! `403 UZ-AUTH-022` naming the required set. An empty requirement means
//! "authenticated-only" (no capability scope) → allowed once a principal exists.
//!
//! Fail-closed: an absent/unknown scope claim parses to the empty set, so every
//! non-empty requirement is denied. If `ctx.principal == null` (composition bug
//! — no auth middleware ran earlier) we short-circuit 401 rather than grant.

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const scopes = @import("../scopes.zig");

const log = logging.scoped(.auth);

pub const AuthCtx = auth_ctx.AuthCtx;

const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing token";
const S_REQUIRES_PREFIX = "Requires scope ";
const S_OR = " or ";
const S_DENIED = "Insufficient scope for this action.";

pub const RequireScope = struct {
    const Self = @This();

    pub fn middleware(self: *Self) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, _: *httpz.Request) anyerror!chain.Outcome {
        const self: *RequireScope = @ptrCast(@alignCast(ptr));
        return execute(self, ctx);
    }

    pub fn execute(_: *Self, ctx: *AuthCtx) !chain.Outcome {
        const principal = ctx.principal orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };
        if (scopes.satisfiesAny(principal.scopes, ctx.required_scopes)) return .next;

        var detail_buf: [256]u8 = undefined;
        const detail = formatRequired(&detail_buf, ctx.required_scopes);
        log.warn("scope_denied", .{
            .req_id = ctx.req_id,
            .error_code = errors.ERR_INSUFFICIENT_SCOPE,
            .sub = principal.user_id orelse "unknown",
            .required = detail,
        });
        ctx.fail(errors.ERR_INSUFFICIENT_SCOPE, detail);
        return .short_circuit;
    }
};

/// Render "Requires scope a or b or c" into a stack buffer, naming the missing
/// capability (Failure Mode "Missing scope"). Falls back to a static string if
/// the set is unexpectedly large for the buffer.
fn formatRequired(buf: []u8, required: []const scopes.Scope) []const u8 {
    if (required.len == 0) return S_DENIED;
    var len = appendStr(buf, 0, S_REQUIRES_PREFIX) orelse return S_DENIED;
    for (required, 0..) |s, i| {
        if (i > 0) len = appendStr(buf, len, S_OR) orelse return S_DENIED;
        len = appendStr(buf, len, s.wire()) orelse return S_DENIED;
    }
    len = appendStr(buf, len, ".") orelse return S_DENIED;
    return buf[0..len];
}

fn appendStr(buf: []u8, off: usize, s: []const u8) ?usize {
    if (off + s.len > buf.len) return null;
    @memcpy(buf[off..][0..s.len], s);
    return off + s.len;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;
const principal_mod = @import("../principal.zig");

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var last_detail: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        last_detail = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, detail: []const u8, _: []const u8) void {
        last_code = code;
        last_detail = detail;
        write_count += 1;
    }
};

fn makeCtx(res: *httpz.Response, principal: ?principal_mod.AuthPrincipal, required: []const scopes.Scope) AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .principal = principal,
        .required_scopes = required,
        .write_error = test_fixtures.writeError,
    };
}

test "require_scope .next when principal holds the required scope (any-of)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const required = [_]scopes.Scope{ .fleet_read, .fleet_write, .fleet_admin };
    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .scopes = scopes.parseClaim("fleet:read") }, &required);
    try testing.expectEqual(chain.Outcome.next, try mw.execute(&ctx));
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
}

test "require_scope .next via hierarchy — fleet:admin satisfies a fleet:read route" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const required = [_]scopes.Scope{.fleet_read};
    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .scopes = scopes.parseClaim("fleet:admin") }, &required);
    try testing.expectEqual(chain.Outcome.next, try mw.execute(&ctx));
}

test "require_scope .next for an authenticated-only route (empty requirement)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .scopes = scopes.Set.initEmpty() }, &[_]scopes.Scope{});
    try testing.expectEqual(chain.Outcome.next, try mw.execute(&ctx));
}

test "require_scope 403 fail-closed when scope set is empty and detail names the scope" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const required = [_]scopes.Scope{.fleet_admin};
    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .scopes = scopes.Set.initEmpty() }, &required);
    try testing.expectEqual(chain.Outcome.short_circuit, try mw.execute(&ctx));
    try testing.expectEqualStrings(errors.ERR_INSUFFICIENT_SCOPE, test_fixtures.last_code);
    try testing.expectEqualStrings("Requires scope fleet:admin.", test_fixtures.last_detail);
}

test "require_scope 403 when a write-holder hits a DELETE demanding :admin" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const required = [_]scopes.Scope{.fleet_admin};
    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .scopes = scopes.parseClaim("fleet:write") }, &required);
    try testing.expectEqual(chain.Outcome.short_circuit, try mw.execute(&ctx));
    try testing.expectEqualStrings(errors.ERR_INSUFFICIENT_SCOPE, test_fixtures.last_code);
}

test "require_scope 401 when no principal present (composition bug)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const required = [_]scopes.Scope{.fleet_read};
    var mw = RequireScope{};
    var ctx = makeCtx(ht.res, null, &required);
    try testing.expectEqual(chain.Outcome.short_circuit, try mw.execute(&ctx));
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "formatRequired joins multiple scopes with ' or '" {
    var buf: [256]u8 = undefined;
    const required = [_]scopes.Scope{ .fleet_read, .fleet_write, .fleet_admin };
    try testing.expectEqualStrings("Requires scope fleet:read or fleet:write or fleet:admin.", formatRequired(&buf, &required));
}
