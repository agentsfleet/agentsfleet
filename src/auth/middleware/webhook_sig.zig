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
const hs = @import("hmac_sig");

const AuthCtx = auth_ctx.AuthCtx;

const log = std.log.scoped(.webhook_sig);

const BEARER_PREFIX: []const u8 = "Bearer ";

/// Mirror of the fields `src/zombie/webhook_verify.VerifyConfig` needs at
/// verify time. Local to `src/auth/` to preserve the `test-auth` portability
/// gate; host populates these in `serve_webhook_lookup.zig` by translating
/// from the canonical registry entry.
pub const SignatureScheme = struct {
    sig_header: []const u8,
    ts_header: ?[]const u8 = null,
    prefix: []const u8,
    hmac_version: []const u8 = "",
    includes_timestamp: bool = false,
    max_ts_drift_seconds: i64,
};

/// Owned result from the host-supplied lookup callback.
/// Caller frees owned slices via `alloc`.
pub const LookupResult = struct {
    expected_secret: ?[]const u8,
    expected_token: ?[]const u8,
    signature_scheme: ?SignatureScheme = null,
    signature_secret: ?[]const u8 = null,

    pub fn deinit(self: LookupResult, alloc: std.mem.Allocator) void {
        if (self.expected_secret) |s| alloc.free(s);
        if (self.expected_token) |t| alloc.free(t);
        if (self.signature_scheme) |s| {
            alloc.free(s.sig_header);
            alloc.free(s.prefix);
            if (s.ts_header) |t| alloc.free(t);
            alloc.free(s.hmac_version);
        }
        if (self.signature_secret) |s| alloc.free(s);
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

            const result_opt = self.lookup_fn(self.lookup_ctx, zombie_id, ctx.alloc) catch |err| {
                log.warn("webhook_sig lookup failed req_id={s} zombie_id={s} err={s}", .{ ctx.req_id, zombie_id, @errorName(err) });
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
                if (!hs.constantTimeEql(provided, expected)) {
                    ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                    return .short_circuit;
                }
                return .next;
            }

            // Strategy 2: Per-zombie HMAC signature (§3) — when a scheme is
            // declared, HMAC is the ONLY acceptable auth path. No silent
            // downgrade to Bearer on vault failure or missing sig_header:
            // both fail closed. A scheme-configured zombie that authenticated
            // via Bearer alone would be a header-omission bypass.
            if (result.signature_scheme) |scheme| {
                const secret = result.signature_secret orelse {
                    log.warn("webhook_sig hmac secret unavailable req_id={s} zombie_id={s} (scheme configured; vault load failed or empty)", .{ ctx.req_id, zombie_id });
                    ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
                    return .short_circuit;
                };
                const provided_sig = req.header(scheme.sig_header) orelse {
                    ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
                    return .short_circuit;
                };
                return verifyHmac(ctx, scheme, secret, provided_sig, req);
            }

            // Strategy 3: Bearer token fallback (reached only when no HMAC
            // scheme is configured on the zombie).
            const expected_token = result.expected_token orelse {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            };
            return verifyBearer(ctx, expected_token, req);
        }
    };
}

fn verifyHmac(
    ctx: *AuthCtx,
    scheme: SignatureScheme,
    secret: []const u8,
    provided_sig: []const u8,
    req: *httpz.Request,
) chain.Outcome {
    // Defense-in-depth: empty key → HMAC is deterministic and attacker-
    // computable. Mirrors the same guard in svix_signature.zig. The parser +
    // execute() already reject empty secrets upstream, but middleware must
    // not trust the vault blindly.
    if (secret.len == 0) {
        ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
        return .short_circuit;
    }
    const timestamp = if (scheme.ts_header) |ts_h| req.header(ts_h) else null;
    if (scheme.ts_header != null) {
        const ts = timestamp orelse {
            ctx.fail(errors.ERR_WEBHOOK_TIMESTAMP_STALE, "Missing signature timestamp");
            return .short_circuit;
        };
        if (!hs.isTimestampFresh(ts, scheme.max_ts_drift_seconds)) {
            ctx.fail(errors.ERR_WEBHOOK_TIMESTAMP_STALE, "Signature timestamp too old");
            return .short_circuit;
        }
    }

    if (!std.mem.startsWith(u8, provided_sig, scheme.prefix)) {
        ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
        return .short_circuit;
    }
    const expected = hs.hexDecode32(provided_sig[scheme.prefix.len..]) orelse {
        ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
        return .short_circuit;
    };
    const body = req.body() orelse "";
    const mac = if (scheme.includes_timestamp) blk: {
        const ts = timestamp orelse {
            ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
            return .short_circuit;
        };
        break :blk hs.computeMac(secret, &.{ scheme.hmac_version, ":", ts, ":", body });
    } else hs.computeMac(secret, &.{body});

    if (!hs.constantTimeEql(&mac, &expected)) {
        ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
        return .short_circuit;
    }
    return .next;
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
    if (!hs.constantTimeEql(provided, expected)) {
        ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
        return .short_circuit;
    }
    return .next;
}

// constantTimeEql moved to src/crypto/hmac_sig.zig (canonical source).

test {
    _ = @import("webhook_sig_test.zig");
}
