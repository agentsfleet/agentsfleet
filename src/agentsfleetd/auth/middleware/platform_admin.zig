//! `platform_admin` middleware.
//!
//! Composes *after* an auth middleware that populated `ctx.principal`.
//! Rejects with 403 `UZ-AUTH-021` unless the principal carries the verified
//! `platform_admin` claim — the one principal agentsfleet allows to mint a
//! runner token (`POST /v1/runners`).
//!
//! Fail-closed twice over: the api_key path never sets `platform_admin` (so a
//! `agt_t` admin key is rejected), and a missing JWT claim parses to false.
//! If `ctx.principal == null` (composition bug — no auth middleware ran
//! earlier in the chain) we short-circuit 401 rather than grant access.

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");

const log = logging.scoped(.auth);

pub const AuthCtx = auth_ctx.AuthCtx;

const DETAIL_REQUIRED = "Platform-admin privileges are required to perform this action.";
const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing token";

pub const PlatformAdmin = struct {
    pub fn middleware(self: *PlatformAdmin) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, _: *httpz.Request) anyerror!chain.Outcome {
        const self: *PlatformAdmin = @ptrCast(@alignCast(ptr));
        return execute(self, ctx);
    }

    pub fn execute(_: *PlatformAdmin, ctx: *AuthCtx) !chain.Outcome {
        const principal = ctx.principal orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };
        if (principal.platform_admin) return .next;

        log.warn("platform_admin_denied", .{
            .req_id = ctx.req_id,
            .error_code = errors.ERR_PLATFORM_ADMIN_REQUIRED,
            .sub = principal.user_id orelse "unknown",
        });
        ctx.fail(errors.ERR_PLATFORM_ADMIN_REQUIRED, DETAIL_REQUIRED);
        return .short_circuit;
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

fn makeCtx(res: *httpz.Response, principal: ?auth_ctx.AuthPrincipal) AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .principal = principal,
        .write_error = test_fixtures.writeError,
    };
}

test "platform_admin .next when principal carries the verified claim" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = PlatformAdmin{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .role = .admin, .platform_admin = true });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
}

test "platform_admin short-circuits 403 when the claim is absent (fail-closed)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // A tenant admin JWT with no platform_admin claim — defaults false.
    var mw = PlatformAdmin{};
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .role = .admin });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_PLATFORM_ADMIN_REQUIRED, test_fixtures.last_code);
}

test "platform_admin short-circuits 403 for an api_key principal" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // A `agt_t` api_key authenticates as .role=.admin but never carries
    // platform_admin — it must not be able to enroll a runner.
    var mw = PlatformAdmin{};
    var ctx = makeCtx(ht.res, .{ .mode = .api_key, .role = .admin });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_PLATFORM_ADMIN_REQUIRED, test_fixtures.last_code);
}

test "platform_admin short-circuits 401 when no principal present (composition bug)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = PlatformAdmin{};
    var ctx = makeCtx(ht.res, null);
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}
