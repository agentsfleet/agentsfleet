//! `webhook_sig` middleware (M28_001).
//!
//! Unified webhook auth: URL-embedded secret → Bearer token fallback.
//! All comparisons use constant-time XOR (RULE CTM + RULE CTC).
//!
//! Generic over `LookupCtx` so the host passes a concrete type (e.g.
//! `*pg.Pool`) instead of `*anyopaque`. The only `anyopaque` remaining
//! is in `executeTypeErased` — required by chain.Middleware's type-erased
//! function pointer interface.
//!
//! Auth strategy resolution order:
//!   1. URL-embedded secret (webhook_provided_secret vs vault-backed expected)
//!   2. Bearer token fallback (Authorization header vs config_json trigger.token)

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");

const AuthCtx = auth_ctx.AuthCtx;

const BEARER_PREFIX: []const u8 = "Bearer ";

/// Owned result from the host-supplied lookup callback.
/// Caller frees owned slices via `alloc`.
pub const LookupResult = struct {
    expected_secret: ?[]const u8,
    expected_token: ?[]const u8,

    pub fn deinit(self: LookupResult, alloc: std.mem.Allocator) void {
        if (self.expected_secret) |s| alloc.free(s);
        if (self.expected_token) |t| alloc.free(t);
    }
};

/// Comptime-generic webhook auth middleware. `LookupCtx` is the concrete
/// type of the host-supplied context (e.g. `*pg.Pool`), avoiding unsafe
/// `*anyopaque` casts in the lookup callback.
pub fn WebhookSig(comptime LookupCtx: type) type {
    return struct {
        const Self = @This();

        lookup_ctx: LookupCtx,
        lookup_fn: *const fn (LookupCtx, []const u8, std.mem.Allocator) anyerror!?LookupResult,

        pub fn middleware(self: *Self) chain.Middleware(AuthCtx) {
            return .{ .ptr = self, .execute_fn = executeTypeErased };
        }

        fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
            const self: *Self = @ptrCast(@alignCast(ptr));
            return self.execute(ctx, req);
        }

        pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
            const zombie_id = ctx.webhook_zombie_id orelse {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            };

            const result_opt = self.lookup_fn(self.lookup_ctx, zombie_id, ctx.alloc) catch {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            };
            const result = result_opt orelse {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            };
            defer result.deinit(ctx.alloc);

            // Strategy 1: URL-embedded secret
            if (ctx.webhook_provided_secret) |provided| {
                const expected = result.expected_secret orelse {
                    ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                    return .short_circuit;
                };
                if (!constantTimeEq(provided, expected)) {
                    ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                    return .short_circuit;
                }
                return .next;
            }

            // Strategy 2: Bearer token fallback
            const expected_token = result.expected_token orelse {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            };
            return verifyBearer(ctx, expected_token, req);
        }
    };
}

fn verifyBearer(ctx: *AuthCtx, expected: []const u8, req: *httpz.Request) chain.Outcome {
    const auth_header = req.header("authorization") orelse {
        ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
        return .short_circuit;
    };
    if (!std.mem.startsWith(u8, auth_header, BEARER_PREFIX)) {
        ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
        return .short_circuit;
    }
    const provided = auth_header[BEARER_PREFIX.len..];
    if (!constantTimeEq(provided, expected)) {
        ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
        return .short_circuit;
    }
    return .next;
}

/// Constant-time byte-slice equality (RULE CTM + RULE CTC).
/// XOR loop runs over min(a.len, b.len); length mismatch folded after.
fn constantTimeEq(a: []const u8, b: []const u8) bool {
    const min_len = @min(a.len, b.len);
    var diff: u8 = 0;
    for (a[0..min_len], b[0..min_len]) |x, y| diff |= x ^ y;
    diff |= @as(u8, @intFromBool(a.len != b.len));
    return diff == 0;
}

// ── Inline tests (same-file access to private fn) ─────────────────────

test "constantTimeEq: equal" {
    try @import("std").testing.expect(constantTimeEq("secret", "secret"));
}

test "constantTimeEq: different value" {
    try @import("std").testing.expect(!constantTimeEq("secret", "wrong!"));
}

test "constantTimeEq: different length" {
    try @import("std").testing.expect(!constantTimeEq("abc", "abcd"));
}

test "constantTimeEq: both empty" {
    try @import("std").testing.expect(constantTimeEq("", ""));
}

test {
    _ = @import("webhook_sig_test.zig");
}
