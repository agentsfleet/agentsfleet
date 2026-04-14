//! `admin_api_key` middleware (M18_002 §3.1).
//!
//! Matches the request's `Authorization: Bearer <token>` against a configured
//! rotation of admin API keys. On match, populates `ctx.principal` as admin;
//! otherwise short-circuits with a 401 (`UZ-AUTH-002`).
//!
//! NOTE: the spec draft describes an `X-API-Key` header — the current wire
//! protocol reuses `Authorization: Bearer` for API keys (same slot as JWTs),
//! so this implementation follows the deployed convention. Changing the
//! header would break existing CLI + operator clients.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const principal_mod = @import("../principal.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const AdminApiKey = struct {
    /// Comma-separated rotation of configured admin keys (already in memory
    /// from the runtime config — not re-read per request).
    api_keys: []const u8,

    pub fn middleware(self: *AdminApiKey) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *AdminApiKey = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *AdminApiKey, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        if (!bearer.matchRotatedKey(provided, self.api_keys)) {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        }
        ctx.principal = .{
            .mode = .api_key,
            .role = .admin,
        };
        return .next;
    }
};

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, _: []const u8, _: []const u8) void {
        last_code = code;
        write_count += 1;
    }
};

fn runOne(mw: *AdminApiKey, ht: anytype) !struct { outcome: chain.Outcome, ctx: AuthCtx } {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    const outcome = try mw.execute(&ctx, ht.req);
    return .{ .outcome = outcome, .ctx = ctx };
}

test "admin_api_key .next + admin principal on matching Bearer key" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer key-b");

    var mw = AdminApiKey{ .api_keys = "key-a, key-b, key-c" };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expect(result.ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.api_key, result.ctx.principal.?.mode);
    try testing.expectEqual(principal_mod.AuthRole.admin, result.ctx.principal.?.role);
}

test "admin_api_key short-circuits with 401 when Authorization header is missing" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // No Authorization header set.

    var mw = AdminApiKey{ .api_keys = "key-a" };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqual(@as(usize, 1), test_fixtures.write_count);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expect(result.ctx.principal == null);
}

test "admin_api_key short-circuits with 401 when Bearer token does not match rotation" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer key-z");

    var mw = AdminApiKey{ .api_keys = "key-a, key-b" };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqual(@as(usize, 1), test_fixtures.write_count);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "admin_api_key rejects Authorization header without Bearer prefix" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "key-a");

    var mw = AdminApiKey{ .api_keys = "key-a" };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}
