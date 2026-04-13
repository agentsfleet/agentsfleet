//! `require_role` middleware (M18_002 §3.2).
//!
//! Composes *after* an auth middleware that populated `ctx.principal`.
//! Rejects with 403 `UZ-AUTH-009` when the principal's role is below the
//! required role; lets the chain continue otherwise.
//!
//! If `ctx.principal == null` (composition bug — no auth middleware ran
//! earlier in the chain) we short-circuit with 401 rather than panic:
//! a misconfigured route must never silently grant access.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const rbac = @import("../rbac.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const RequireRole = struct {
    required: rbac.AuthRole,

    pub fn middleware(self: *RequireRole) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, _: *httpz.Request) anyerror!chain.Outcome {
        const self: *RequireRole = @ptrCast(@alignCast(ptr));
        return execute(self, ctx);
    }

    pub fn execute(self: *RequireRole, ctx: *AuthCtx) !chain.Outcome {
        const principal = ctx.principal orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        if (principal.role.allows(self.required)) return .next;

        var detail_buf: [128]u8 = undefined;
        const detail = std.fmt.bufPrint(
            &detail_buf,
            "Your role is '{s}'. {s} role required.",
            .{ principal.role.label(), self.required.label() },
        ) catch "Insufficient role";
        ctx.fail(errors.ERR_INSUFFICIENT_ROLE, detail);
        return .short_circuit;
    }
};

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;
const principal_mod = @import("../principal.zig");

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

fn makeCtx(res: *httpz.Response, principal: ?principal_mod.AuthPrincipal) AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .principal = principal,
        .write_error = test_fixtures.writeError,
    };
}

test "require_role .next when principal role meets required (admin ≥ admin)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = RequireRole{ .required = .admin };
    var ctx = makeCtx(ht.res, .{ .mode = .api_key, .role = .admin });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
}

test "require_role .next when principal role exceeds required (admin ≥ operator)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = RequireRole{ .required = .operator };
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .role = .admin });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.next, outcome);
}

test "require_role short-circuits with 403 when role is below required (user < admin)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = RequireRole{ .required = .admin };
    var ctx = makeCtx(ht.res, .{ .mode = .jwt_oidc, .role = .user });
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_INSUFFICIENT_ROLE, test_fixtures.last_code);
}

test "require_role short-circuits with 401 when no principal present (composition bug)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = RequireRole{ .required = .admin };
    var ctx = makeCtx(ht.res, null);
    const outcome = try mw.execute(&ctx);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}
