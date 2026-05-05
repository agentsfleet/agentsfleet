//! `svix_signature` middleware (M28_001 §5).
//!
//! Verifies Svix v1 multi-sig HMAC-SHA256 for Clerk (and any future Svix-
//! signed provider) webhooks. Route path `/v1/webhooks/svix/{zombie_id}`
//! selects this middleware statically; no per-request provider switch.
//!
//! Crypto core lives in the sibling `auth/crypto/svix_verify.zig` and is
//! shared with the `/v1/webhooks/clerk` handler. This file is a thin adapter
//! that bridges the pure verifier to the middleware chain: vault secret
//! lookup, `*AuthCtx` fail-write, and the typed-handle pattern.
//!
//! Lives in `src/auth/` under the portability gate — imports only std,
//! httpz, the `hmac_sig` named module, and the sibling `svix_verify` module
//! (which itself imports only std + `hmac_sig`).

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const sv = @import("../crypto/svix_verify.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

// Re-export primitives for callers that still import from this file.
pub const SVIX_ID_HEADER = sv.SVIX_ID_HEADER;
pub const SVIX_TS_HEADER = sv.SVIX_TS_HEADER;
pub const SVIX_SIG_HEADER = sv.SVIX_SIG_HEADER;
pub const SVIX_SECRET_PREFIX = sv.SVIX_SECRET_PREFIX;
pub const SVIX_SIG_VERSION = sv.SVIX_SIG_VERSION;
pub const SVIX_MAX_DRIFT_SECONDS = sv.SVIX_MAX_DRIFT_SECONDS;
pub const isTimestampFresh = sv.isTimestampFresh;

pub const NowSecondsFn = *const fn () i64;

fn defaultNowSeconds() i64 {
    return std.time.timestamp();
}

const log = std.log.scoped(.svix);

/// Owned lookup result. `secret` is the `whsec_<base64>` string loaded from
/// vault for the target zombie — the verifier handles prefix stripping and
/// base64 decoding internally.
pub const SvixLookupResult = struct {
    secret: ?[]const u8,

    pub fn deinit(self: SvixLookupResult, alloc: std.mem.Allocator) void {
        if (self.secret) |s| alloc.free(s);
    }
};

/// Comptime-generic Svix middleware. `LookupCtx` is the concrete type of
/// the host context (e.g. `*pg.Pool`) — no `*anyopaque` in the callback.
pub fn SvixSignature(comptime LookupCtx: type) type {
    return struct {
        const Self = @This();

        lookup_ctx: LookupCtx,
        lookup_fn: *const fn (LookupCtx, []const u8, std.mem.Allocator) anyerror!?SvixLookupResult,
        now_seconds: NowSecondsFn = defaultNowSeconds,

        pub fn middleware(self: *Self) chain.Middleware(AuthCtx) {
            return .{ .ptr = self, .execute_fn = executeTypeErased };
        }

        fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
            const self: *Self = @ptrCast(@alignCast(ptr));
            return self.execute(ctx, req);
        }

        pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
            const zombie_id = ctx.webhook_zombie_id orelse return failSig(ctx);

            const result_opt = self.lookup_fn(self.lookup_ctx, zombie_id, ctx.alloc) catch |err| {
                log.warn("svix lookup failed req_id={s} zombie_id={s} err={s}", .{ ctx.req_id, zombie_id, @errorName(err) });
                return failSig(ctx);
            };
            const result = result_opt orelse return failSig(ctx);
            defer result.deinit(ctx.alloc);

            const secret_raw = result.secret orelse return failSig(ctx);
            return dispatchResult(ctx, req, secret_raw, self.now_seconds());
        }
    };
}

fn failSig(ctx: *AuthCtx) chain.Outcome {
    ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
    return .short_circuit;
}

fn failStale(ctx: *AuthCtx) chain.Outcome {
    ctx.fail(errors.ERR_WEBHOOK_TIMESTAMP_STALE, "Signature timestamp too old");
    return .short_circuit;
}

/// Bridge the pure verifier to the middleware's error-writing surface.
fn dispatchResult(ctx: *AuthCtx, req: *httpz.Request, secret_raw: []const u8, now: i64) chain.Outcome {
    const svix_id = req.header(sv.SVIX_ID_HEADER) orelse return failSig(ctx);
    const svix_ts = req.header(sv.SVIX_TS_HEADER) orelse return failSig(ctx);
    const svix_sig = req.header(sv.SVIX_SIG_HEADER) orelse return failSig(ctx);
    const body = req.body() orelse "";

    const result = sv.verifySvix(secret_raw, svix_id, svix_ts, svix_sig, body, now, sv.SVIX_MAX_DRIFT_SECONDS);
    return switch (result) {
        .ok => .next,
        .stale_timestamp => failStale(ctx),
        .invalid_signature => failSig(ctx),
    };
}

test {
    _ = @import("svix_signature_test.zig");
}
