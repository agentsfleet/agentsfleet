//! `webhook_url_secret` middleware (M18_002 §3.3).
//!
//! Verifies a URL-embedded per-zombie secret. The route carries the secret
//! component in the URL path (`/v1/webhooks/{zombie_id}/{secret}`) and the
//! dispatcher populates `ctx.webhook_zombie_id` / `ctx.webhook_provided_secret`
//! before invoking the chain.
//!
//! The expected secret is loaded on-demand via `lookup_fn` — a host-supplied
//! callback that abstracts the vault/DB lookup. The middleware never touches
//! `src/db/` or `src/errors/` directly (§1.2 portability).
//!
//! Comparison is constant-time to prevent timing oracles on secret length/value.
//!
//! Per-request slot (`ctx.webhook_zombie_id`, `ctx.webhook_provided_secret`) is
//! populated by the dispatcher before calling chain.run for `.receive_webhook`
//! routes. For C.2 (empty route table) this path is never exercised.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

/// Host-supplied vault/DB lookup callback.
///
/// Returns an owned slice with the expected secret — the middleware frees it via
/// `ctx.alloc`. Returns `null` if no secret is configured for the given zombie.
/// Returns an error if the backing store (DB/vault) is unavailable.
pub const LookupFn = *const fn (
    user: *anyopaque,
    zombie_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?[]const u8;

pub const WebhookUrlSecret = struct {
    /// Opaque pointer passed to `lookup_fn` on every call (e.g. a DB pool).
    lookup_ctx: *anyopaque,
    lookup_fn: LookupFn,

    pub fn middleware(self: *WebhookUrlSecret) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        _ = req; // URL-embedded secret is in AuthCtx fields, not in request headers.
        const self: *WebhookUrlSecret = @ptrCast(@alignCast(ptr));
        return execute(self, ctx);
    }

    pub fn execute(self: *WebhookUrlSecret, ctx: *AuthCtx) !chain.Outcome {
        const zombie_id = ctx.webhook_zombie_id orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        const provided = ctx.webhook_provided_secret orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };

        const expected_opt = self.lookup_fn(self.lookup_ctx, zombie_id, ctx.alloc) catch {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        const expected = expected_opt orelse {
            // No secret is configured for this zombie — reject unconditionally.
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        };
        defer ctx.alloc.free(expected);

        if (!constantTimeEq(provided, expected)) {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        }
        return .next;
    }
};

/// Constant-time byte-slice equality.
///
/// Always iterates over the minimum-length prefix so the XOR loop does not
/// short-circuit on mismatch, preventing timing leaks on secret value or length.
fn constantTimeEq(a: []const u8, b: []const u8) bool {
    const min_len = @min(a.len, b.len);
    var diff: u8 = 0;
    for (a[0..min_len], b[0..min_len]) |x, y| diff |= x ^ y;
    // Length mismatch is folded in after the loop (not a conditional branch).
    diff |= @as(u8, @intFromBool(a.len != b.len));
    return diff == 0;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

const Fixtures = struct {
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

const TestLookup = struct {
    var return_secret: ?[]const u8 = null; // if non-null, duplicated via alloc
    var should_fail: bool = false;

    fn reset() void {
        return_secret = null;
        should_fail = false;
    }

    /// Dummy opaque pointer — the function ignores it.
    var dummy: u8 = 0;

    fn lookup(_: *anyopaque, _: []const u8, alloc: std.mem.Allocator) anyerror!?[]const u8 {
        if (should_fail) return error.Unavailable;
        const s = return_secret orelse return null;
        const duped: []const u8 = try alloc.dupe(u8, s);
        return duped;
    }
};

fn makeCtx(res: *httpz.Response, zombie_id: ?[]const u8, provided: ?[]const u8) auth_ctx.AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .write_error = Fixtures.writeError,
        .webhook_zombie_id = zombie_id,
        .webhook_provided_secret = provided,
    };
}

fn runMw(res: *httpz.Response, zombie_id: ?[]const u8, provided: ?[]const u8) !chain.Outcome {
    var mw = WebhookUrlSecret{
        .lookup_ctx = &TestLookup.dummy,
        .lookup_fn = TestLookup.lookup,
    };
    var ctx = makeCtx(res, zombie_id, provided);
    return mw.execute(&ctx);
}

test "webhook_url_secret .next when provided matches expected" {
    Fixtures.reset();
    TestLookup.reset();
    TestLookup.return_secret = "correct-secret";

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, "zombie-abc", "correct-secret");
    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), Fixtures.write_count);
}

test "webhook_url_secret short-circuits with 401 when provided does not match" {
    Fixtures.reset();
    TestLookup.reset();
    TestLookup.return_secret = "correct-secret";

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, "zombie-abc", "wrong-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "webhook_url_secret short-circuits with 401 when zombie_id is missing from ctx" {
    Fixtures.reset();
    TestLookup.reset();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, null, "any-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "webhook_url_secret short-circuits with 401 when provided_secret is missing from ctx" {
    Fixtures.reset();
    TestLookup.reset();
    TestLookup.return_secret = "expected";

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, "zombie-abc", null);
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "webhook_url_secret short-circuits with 401 when no secret is configured (lookup returns null)" {
    Fixtures.reset();
    TestLookup.reset();
    TestLookup.return_secret = null; // not configured

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, "zombie-abc", "any-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "webhook_url_secret short-circuits with 401 when lookup returns an error" {
    Fixtures.reset();
    TestLookup.reset();
    TestLookup.should_fail = true;

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const outcome = try runMw(ht.res, "zombie-abc", "any-secret");
    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, Fixtures.last_code);
}

test "constantTimeEq: equal strings return true" {
    try testing.expect(constantTimeEq("secret-abc", "secret-abc"));
}

test "constantTimeEq: different strings return false without short-circuiting on first byte" {
    try testing.expect(!constantTimeEq("secret-abc", "secret-xyz"));
}

test "constantTimeEq: different lengths return false" {
    try testing.expect(!constantTimeEq("abc", "abcd"));
    try testing.expect(!constantTimeEq("abcd", "abc"));
}

test "constantTimeEq: empty strings are equal" {
    try testing.expect(constantTimeEq("", ""));
}
