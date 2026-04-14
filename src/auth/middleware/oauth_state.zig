//! `oauth_state` middleware (M18_002 §3.3).
//!
//! Verifies an OAuth-callback `state` parameter of the form
//!   `{nonce_hex}.{workspace_id}.{hmac_hex}`
//! against a configured HMAC-SHA256 secret, then atomically consumes the
//! nonce via a host-supplied callback to prevent replay.
//!
//! The nonce-consume callback is abstracted through `*anyopaque + fn` so the
//! concrete Redis client (`queue_redis.Client`) never leaks into `src/auth/`.
//! Callers typically wire `consume_nonce` to a `DEL slack:oauth:nonce:<n>`
//! Redis command — identical to `src/http/handlers/slack_oauth.zig::validateState`.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

/// UZ-SLACK-001 — re-declared locally to avoid importing `src/errors/`.
pub const ERR_OAUTH_STATE_INVALID: []const u8 = "UZ-SLACK-001";

/// Opaque-pointer callback that atomically consumes a nonce. Returns:
///   - `true`: nonce existed and was consumed (state is fresh).
///   - `false`: nonce missing (expired or already replayed).
///   - `error.*`: backing store (Redis/etc.) is unavailable — middleware
///     surfaces as 500-ish, not as an auth failure.
pub const ConsumeNonceFn = *const fn (user: *anyopaque, nonce: []const u8) anyerror!bool;

pub const OAuthState = struct {
    /// Shared signing secret (e.g. `SLACK_SIGNING_SECRET`).
    signing_secret: []const u8,
    /// Host-owned pointer passed to `consume_nonce` on every call
    /// (typically `*queue_redis.Client`).
    consume_ctx: *anyopaque,
    consume_nonce: ConsumeNonceFn,

    pub fn middleware(self: *OAuthState) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *OAuthState = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *OAuthState, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const qs = req.query() catch {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Invalid query string");
            return .short_circuit;
        };
        const state = qs.get("state") orelse {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "state required");
            return .short_circuit;
        };

        // Parse "{nonce}.{workspace_id}.{hmac}" without allocations.
        const first = std.mem.indexOfScalar(u8, state, '.') orelse {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Invalid state");
            return .short_circuit;
        };
        const last = std.mem.lastIndexOfScalar(u8, state, '.') orelse first;
        if (first >= last) {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Invalid state");
            return .short_circuit;
        }
        const nonce = state[0..first];
        const workspace_id = state[first + 1 .. last];
        const provided_hex = state[last + 1 ..];

        if (nonce.len == 0 or workspace_id.len == 0 or provided_hex.len != HmacSha256.mac_length * 2) {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Invalid state HMAC");
            return .short_circuit;
        }

        var expected: [HmacSha256.mac_length]u8 = undefined;
        var h = HmacSha256.init(self.signing_secret);
        h.update(nonce);
        h.update(":");
        h.update(workspace_id);
        h.final(&expected);

        var provided_bytes: [HmacSha256.mac_length]u8 = undefined;
        _ = std.fmt.hexToBytes(&provided_bytes, provided_hex) catch {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Invalid state HMAC");
            return .short_circuit;
        };

        if (!std.crypto.timing_safe.eql([HmacSha256.mac_length]u8, expected, provided_bytes)) {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "OAuth state invalid");
            return .short_circuit;
        }

        // Atomic consume — errors from the backing store surface as a
        // distinct failure detail so operators can separate "replayed state"
        // from "Redis unavailable".
        const consumed = self.consume_nonce(self.consume_ctx, nonce) catch {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "Nonce store unavailable");
            return .short_circuit;
        };
        if (!consumed) {
            ctx.fail(ERR_OAUTH_STATE_INVALID, "OAuth state expired or replayed");
            return .short_circuit;
        }
        return .next;
    }
};

test {
    _ = @import("oauth_state_test.zig");
}
