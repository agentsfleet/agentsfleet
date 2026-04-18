//! `tenant_api_key` middleware (M28_002 §2).
//!
//! Resolves `Authorization: Bearer zmb_t_{hex}` tokens via a host-supplied
//! `LookupFn` callback. On match (and row.active = true), populates
//! `ctx.principal` with `.mode=.api_key`, `.role=.admin`, `.user_id`, and
//! `.tenant_id`. Rejects unknown keys with 401 ERR_UNAUTHORIZED; rejects
//! revoked keys with 401 ERR_APIKEY_REVOKED.
//!
//! Portability: this file MUST NOT import from `src/db/`, `src/http/`, or
//! any business-layer module (§1.2 contract; enforced by `make test-auth`).
//! The DB lookup lives behind `LookupFn`, wired by the host (serve.zig) at
//! boot.
//!
//! Lifetime: `LookupFn` returns slices owned by the caller's allocator. The
//! middleware duplicates the kept fields (`user_id`, `tenant_id`) into
//! `ctx.alloc` and frees the caller's slices before returning. The handler
//! layer then owns the principal fields, freed after the request completes.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const api_key = @import("../api_key.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const TENANT_KEY_PREFIX = "zmb_t_";

// src/auth/ is portable — cannot import from src/errors/. Duplicated here;
// a cross-layer parity test (already in place for the other middleware
// error-code strings) keeps them in sync.
const ERR_APIKEY_REVOKED: []const u8 = "UZ-APIKEY-004";

const log = std.log.scoped(.api_keys);

/// Outcome of a key-hash lookup. All slices are owned by the allocator
/// passed to `LookupFn`; the caller of `LookupFn` is responsible for
/// freeing them.
pub const LookupResult = struct {
    api_key_id: []const u8,
    tenant_id: []const u8,
    user_id: []const u8,
    active: bool,
};

/// Host-supplied callback that resolves a SHA-256 hex digest to a key row.
/// Returns `null` when no row matches the hash. The host is responsible for
/// DB access; `src/auth/` never reaches into `src/db/`.
pub const LookupFn = *const fn (
    host: *anyopaque,
    alloc: std.mem.Allocator,
    key_hash_hex: []const u8,
) anyerror!?LookupResult;

pub const TenantApiKey = struct {
    host: *anyopaque,
    lookup: LookupFn,

    pub fn middleware(self: *TenantApiKey) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *TenantApiKey = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *TenantApiKey, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        if (!std.mem.startsWith(u8, provided, TENANT_KEY_PREFIX)) {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        }
        return resolve(self, ctx, provided);
    }
};

fn resolve(self: *TenantApiKey, ctx: *AuthCtx, raw_key: []const u8) !chain.Outcome {
    const hash_hex = api_key.sha256Hex(raw_key);

    const maybe_row = self.lookup(self.host, ctx.alloc, hash_hex[0..]) catch {
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, "Authentication service unavailable");
        return .short_circuit;
    };
    const row = maybe_row orelse {
        log.info("api_key.auth_rejected reason=unknown key_prefix=" ++ TENANT_KEY_PREFIX, .{});
        ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
        return .short_circuit;
    };

    if (!row.active) {
        log.info("api_key.auth_rejected reason=revoked api_key_id={s}", .{row.api_key_id});
        freeRow(ctx.alloc, row);
        ctx.fail(ERR_APIKEY_REVOKED, "API key has been revoked");
        return .short_circuit;
    }

    log.info("api_key.auth_succeeded api_key_id={s} tenant_id={s}", .{ row.api_key_id, row.tenant_id });
    ctx.alloc.free(row.api_key_id);
    ctx.principal = .{
        .mode = .api_key,
        .role = .admin,
        .user_id = row.user_id,
        .tenant_id = row.tenant_id,
    };
    return .next;
}

fn freeRow(alloc: std.mem.Allocator, row: LookupResult) void {
    alloc.free(row.api_key_id);
    alloc.free(row.tenant_id);
    alloc.free(row.user_id);
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;
const principal_mod = @import("../principal.zig");

const MockLookup = struct {
    want_hash: []const u8 = "",
    return_row: ?LookupResult = null,
    return_err: ?anyerror = null,
    called_with: []const u8 = "",
    call_count: usize = 0,

    fn fn_(host: *anyopaque, alloc: std.mem.Allocator, key_hash_hex: []const u8) anyerror!?LookupResult {
        const self: *MockLookup = @ptrCast(@alignCast(host));
        self.called_with = key_hash_hex;
        self.call_count += 1;
        if (self.return_err) |e| return e;
        if (self.return_row) |row| {
            return .{
                .api_key_id = try alloc.dupe(u8, row.api_key_id),
                .tenant_id = try alloc.dupe(u8, row.tenant_id),
                .user_id = try alloc.dupe(u8, row.user_id),
                .active = row.active,
            };
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

test "tenant_api_key rejects missing Authorization header with UZ-AUTH-002" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mock = MockLookup{};
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqual(@as(usize, 1), test_fixtures.write_count);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects Bearer token without zmb_t_ prefix without calling lookup" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer zmb_notatenantkey");

    var mock = MockLookup{};
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects unknown key with UZ-AUTH-002 and emits rejected log" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer zmb_t_" ++ "0" ** 64);

    var mock = MockLookup{ .return_row = null };
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 1), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects revoked key with UZ-APIKEY-004 and frees row slices" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer zmb_t_" ++ "a" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "33333333-3333-7333-8333-333333333333",
            .active = false,
        },
    };
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(ERR_APIKEY_REVOKED, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key populates principal on active key match" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer zmb_t_" ++ "b" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "33333333-3333-7333-8333-333333333333",
            .active = true,
        },
    };
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expect(ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.api_key, ctx.principal.?.mode);
    try testing.expectEqual(principal_mod.AuthRole.admin, ctx.principal.?.role);
    try testing.expectEqualStrings("33333333-3333-7333-8333-333333333333", ctx.principal.?.user_id.?);
    try testing.expectEqualStrings("22222222-2222-7222-8222-222222222222", ctx.principal.?.tenant_id.?);
}

test "tenant_api_key surfaces LookupFn error as UZ-AUTH-004" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer zmb_t_" ++ "c" ** 64);

    var mock = MockLookup{ .return_err = error.Unexpected };
    var mw = TenantApiKey{ .host = &mock, .lookup = MockLookup.fn_ };
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "TENANT_KEY_PREFIX is the documented zmb_t_ literal" {
    try testing.expectEqualStrings("zmb_t_", TENANT_KEY_PREFIX);
}
