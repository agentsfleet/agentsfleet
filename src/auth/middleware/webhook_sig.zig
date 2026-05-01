//! `webhook_sig` middleware — per-zombie webhook HMAC auth.
//!
//! HMAC-SHA256 over the raw body is the ONLY acceptable auth path. The
//! resolver's job is to return the scheme + secret for the zombie's
//! configured provider. Three failure modes, all fail-closed:
//!
//!   - Provider not recognized / no scheme configured →
//!     `UZ-WH-020 webhook_credential_not_configured`.
//!   - Scheme present, vault credential missing or empty →
//!     `UZ-WH-020 webhook_credential_not_configured`.
//!   - Scheme + secret present, signature header missing/malformed/wrong →
//!     `UZ-WH-010 invalid signature` (or `UZ-WH-011 stale timestamp`).
//!
//! No Bearer fallback. Pre-v2.0 contract: every webhook is HMAC-signed.
//!
//! All comparisons are constant-time (RULE CTM + RULE CTC).
//!
//! Generic over `LookupCtx` so the host passes a concrete type (e.g.
//! `*pg.Pool`) instead of `*anyopaque`. The only `anyopaque` remaining
//! is in `executeTypeErased` — required by chain.Middleware's type-erased
//! function pointer interface.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const hs = @import("hmac_sig");

const AuthCtx = auth_ctx.AuthCtx;

const log = std.log.scoped(.webhook_sig);

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

/// Owned result from the host-supplied lookup callback. The resolver MUST
/// set `signature_scheme` for any zombie configured with a recognized
/// provider — even when the vault secret is missing — so the middleware
/// fails closed via `UZ-WH-020` instead of silently rejecting as
/// "unauthorized." Leaving `signature_scheme` null is reserved for "no
/// provider configured at all," which also rejects but with a different
/// error code path.
///
/// Caller frees owned slices via `alloc`.
pub const LookupResult = struct {
    signature_scheme: ?SignatureScheme = null,
    signature_secret: ?[]const u8 = null,

    pub fn deinit(self: LookupResult, alloc: std.mem.Allocator) void {
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
                ctx.fail(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, "Webhook credential not configured");
                return .short_circuit;
            };

            const result_opt = self.lookup_fn(self.lookup_ctx, zombie_id, ctx.alloc) catch |err| {
                log.warn("webhook_sig lookup failed req_id={s} zombie_id={s} err={s}", .{ ctx.req_id, zombie_id, @errorName(err) });
                ctx.fail(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, "Webhook credential not configured");
                return .short_circuit;
            };
            const result = result_opt orelse {
                ctx.fail(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, "Webhook credential not configured");
                return .short_circuit;
            };
            defer result.deinit(ctx.alloc);

            // No scheme = no provider configured. Reject as "credential not
            // configured" so operators see a recoverable error class.
            const scheme = result.signature_scheme orelse {
                log.warn("webhook_sig no scheme req_id={s} zombie_id={s}", .{ ctx.req_id, zombie_id });
                ctx.fail(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, "Webhook credential not configured");
                return .short_circuit;
            };
            // Scheme but no secret = vault row missing or malformed. Distinct
            // from "signature wrong" — this is a recoverable misconfiguration,
            // not an attack.
            const secret = result.signature_secret orelse {
                log.warn("webhook_sig hmac secret unavailable req_id={s} zombie_id={s} (vault load failed or empty)", .{ ctx.req_id, zombie_id });
                ctx.fail(errors.ERR_WEBHOOK_CREDENTIAL_NOT_CONFIGURED, "Webhook credential not configured");
                return .short_circuit;
            };
            const provided_sig = req.header(scheme.sig_header) orelse {
                ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
                return .short_circuit;
            };
            return verifyHmac(ctx, scheme, secret, provided_sig, req);
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

// constantTimeEql moved to src/crypto/hmac_sig.zig (canonical source).

test {
    _ = @import("webhook_sig_test.zig");
}
