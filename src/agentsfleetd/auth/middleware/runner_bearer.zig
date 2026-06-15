//! `runnerBearer` middleware — the machine-principal auth plane.
//!
//! Validates `Authorization: Bearer agt_r{hex}` runner tokens via a
//! host-supplied `LookupFn` that hashes the token and resolves it against
//! `fleet.runners`. On an active match, populates `ctx.principal` with
//! `.mode = .runner`, `.runner_id`, and `.tenant_id = null` — a runner holds
//! no tenant authority of its own (secret delivery is placement, not a
//! standing grant). Unknown/malformed tokens reject as `UZ-RUN-001`; known
//! runners with non-active admin state reject as `UZ-RUN-009`;
//! there is no JWKS fall-through, so a runner token can never satisfy a
//! tenant route and a tenant/user token can never satisfy a runner route —
//! the boundary is which middleware guards the route, not a per-handler check.
//! See `docs/AUTH.md` (Runner token) and `docs/architecture/runner_fleet.md`.
//!
//! Portability: like the other `src/auth/middleware/` files, this MUST NOT
//! import from `src/db/`, `src/http/`, or any business-layer module. The DB
//! lookup lives behind `LookupFn`, wired by the host (serve.zig) at boot.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const api_key = @import("../api_key.zig");
const logging = @import("log");
const contract = @import("contract");

pub const AuthCtx = auth_ctx.AuthCtx;

/// The runner-token prefix. Single-sourced in the shared `contract` module
/// (RULE UFS) so the host daemon's config validator references the same literal;
/// re-exported here for the register handler + this middleware. Pin-tested below.
pub const RUNNER_TOKEN_PREFIX = contract.protocol.RUNNER_TOKEN_PREFIX;

const log = logging.scoped(.runner_auth);

const S_AUTH_REJECTED = "auth_rejected";
const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing runner token";
const S_RUNNER_ADMIN_STATE_BLOCKED = "Runner admin state blocks runner-plane access";

/// Outcome of a token-hash lookup against `fleet.runners`. `runner_id` is
/// owned by the allocator passed to `LookupFn`; the caller frees it — the
/// middleware on reject paths, the principal lifecycle on success.
pub const LookupResult = struct {
    runner_id: []const u8,
    active: bool,
};

/// Host-supplied callback resolving a SHA-256 hex digest to a runner row.
/// Returns `null` when no row matches. `src/auth/` never reaches `src/db/`.
pub const LookupFn = *const fn (
    host: *anyopaque,
    alloc: std.mem.Allocator,
    token_hash_hex: []const u8,
) anyerror!?LookupResult;

pub const RunnerBearer = struct {
    host: *anyopaque,
    lookup: LookupFn,

    pub fn middleware(self: *RunnerBearer) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *RunnerBearer = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *RunnerBearer, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_RUN_INVALID_RUNNER_TOKEN, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };
        if (!std.mem.startsWith(u8, provided, RUNNER_TOKEN_PREFIX)) {
            ctx.fail(errors.ERR_RUN_INVALID_RUNNER_TOKEN, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        }
        return resolve(self, ctx, provided);
    }
};

fn resolve(self: *RunnerBearer, ctx: *AuthCtx, raw_token: []const u8) !chain.Outcome {
    const hash_hex = api_key.sha256Hex(raw_token);

    const maybe_row = self.lookup(self.host, ctx.alloc, hash_hex[0..]) catch {
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, "Authentication service unavailable");
        return .short_circuit;
    };
    const row = maybe_row orelse {
        log.info(S_AUTH_REJECTED, .{ .reason = "unknown" });
        ctx.fail(errors.ERR_RUN_INVALID_RUNNER_TOKEN, S_INVALID_OR_MISSING_TOKEN);
        return .short_circuit;
    };

    if (!row.active) {
        log.info(S_AUTH_REJECTED, .{ .reason = "non_active", .runner_id = row.runner_id });
        ctx.alloc.free(row.runner_id);
        ctx.fail(errors.ERR_RUN_ADMIN_STATE_BLOCKED, S_RUNNER_ADMIN_STATE_BLOCKED);
        return .short_circuit;
    }

    log.debug("auth_succeeded", .{ .runner_id = row.runner_id });
    ctx.principal = .{
        .mode = .runner,
        .runner_id = row.runner_id,
        .tenant_id = null,
    };
    return .next;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;
const principal_mod = @import("../principal.zig");

const MockLookup = struct {
    return_row: ?LookupResult = null,
    return_err: ?anyerror = null,
    call_count: usize = 0,

    fn fn_(host: *anyopaque, alloc: std.mem.Allocator, token_hash_hex: []const u8) anyerror!?LookupResult {
        const self: *MockLookup = @ptrCast(@alignCast(host));
        _ = token_hash_hex;
        self.call_count += 1;
        if (self.return_err) |e| return e;
        if (self.return_row) |row| {
            return .{ .runner_id = try alloc.dupe(u8, row.runner_id), .active = row.active };
        }
        return null;
    }
};

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

fn makeCtx(res: *httpz.Response) AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
}

test "runner_bearer rejects missing Authorization header with UZ-RUN-001 without calling lookup" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mock = MockLookup{};
    var mw = RunnerBearer{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_RUN_INVALID_RUNNER_TOKEN, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "runner_bearer rejects Bearer token without agt_r prefix without calling lookup" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_tnotarunner");

    var mock = MockLookup{};
    var mw = RunnerBearer{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_RUN_INVALID_RUNNER_TOKEN, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "runner_bearer rejects unknown token with UZ-RUN-001" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_r" ++ "0" ** 64);

    var mock = MockLookup{ .return_row = null };
    var mw = RunnerBearer{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_RUN_INVALID_RUNNER_TOKEN, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 1), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "runner_bearer rejects revoked runner with UZ-RUN-009 and frees the row" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_r" ++ "a" ** 64);

    var mock = MockLookup{
        .return_row = .{ .runner_id = "11111111-1111-7111-8111-111111111111", .active = false },
    };
    var mw = RunnerBearer{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_RUN_ADMIN_STATE_BLOCKED, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "runner_bearer populates a runner principal on active match" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_r" ++ "b" ** 64);

    var mock = MockLookup{
        .return_row = .{ .runner_id = "22222222-2222-7222-8222-222222222222", .active = true },
    };
    var mw = RunnerBearer{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.runner_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expect(ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.runner, ctx.principal.?.mode);
    try testing.expectEqualStrings("22222222-2222-7222-8222-222222222222", ctx.principal.?.runner_id.?);
    try testing.expect(ctx.principal.?.tenant_id == null);
}

test "RUNNER_TOKEN_PREFIX is the documented agt_r literal" {
    try testing.expectEqualStrings("agt_r", RUNNER_TOKEN_PREFIX);
}
