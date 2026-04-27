//! `slack_signature` middleware (M18_002 §3.3).
//!
//! Verifies Slack's `x-slack-signature` + `x-slack-request-timestamp`
//! headers using HMAC-SHA256 over `"v0:<ts>:<body>"`, with a 300s drift
//! window to prevent replay. Signature format: `"v0=" ++ hex(mac)`.
//!
//! Mirrors the deployed verification logic in `slack_events.zig` and
//! `slack_interactions.zig` so those handlers can migrate in Batch D
//! without behavior change. Config-driven values (header names, prefix,
//! max drift) live in `src/zombie/webhook_verify.zig`; we inline the
//! Slack-specific constants here to keep `src/auth/` portable.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const hs = @import("hmac_sig");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const SLACK_SIG_HEADER: []const u8 = "x-slack-signature";
pub const SLACK_TS_HEADER: []const u8 = "x-slack-request-timestamp";
const SLACK_SIG_PREFIX: []const u8 = "v0=";
pub const SLACK_SIG_VERSION: []const u8 = "v0";
const SLACK_MAX_DRIFT_SECONDS: i64 = 300;

/// Seconds-since-epoch provider; swapped in tests.
pub const NowSecondsFn = *const fn () i64;

fn defaultNowSeconds() i64 {
    return std.time.timestamp();
}

pub const SlackSignature = struct {
    /// Slack app signing secret (raw bytes, not base64). Host loads from env
    /// or vault at boot and passes a stable slice in.
    secret: []const u8,
    now_seconds: NowSecondsFn = defaultNowSeconds,

    pub fn middleware(self: *SlackSignature) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *SlackSignature = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *SlackSignature, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const timestamp = req.header(SLACK_TS_HEADER) orelse {
            ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Missing timestamp header");
            return .short_circuit;
        };
        const signature = req.header(SLACK_SIG_HEADER) orelse {
            ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Missing signature header");
            return .short_circuit;
        };

        if (!isTimestampFresh(timestamp, self.now_seconds(), SLACK_MAX_DRIFT_SECONDS)) {
            ctx.fail(errors.ERR_WEBHOOK_TIMESTAMP_STALE, "Timestamp too old");
            return .short_circuit;
        }

        const body = req.body() orelse "";
        if (!verifyV0Signature(self.secret, timestamp, body, signature)) {
            ctx.fail(errors.ERR_WEBHOOK_SIG_INVALID, "Invalid signature");
            return .short_circuit;
        }
        return .next;
    }
};

/// Verify `"v0=" ++ hex(HmacSha256(secret, "v0:<ts>:<body>"))`.
/// Rejects missing prefix, wrong hex length, and mismatched MAC bytes in
/// constant time.
fn verifyV0Signature(
    secret: []const u8,
    timestamp: []const u8,
    body: []const u8,
    signature: []const u8,
) bool {
    if (!std.mem.startsWith(u8, signature, SLACK_SIG_PREFIX)) return false;
    const expected = hs.hexDecode32(signature[SLACK_SIG_PREFIX.len..]) orelse return false;
    const mac = hs.computeMac(secret, &.{ SLACK_SIG_VERSION, ":", timestamp, ":", body });
    return hs.constantTimeEql(&mac, &expected);
}

/// Freshness window check: rejects timestamps outside `[now - max_drift,
/// now + max_drift]`. Future tolerance is bounded to prevent pre-signed
/// request attacks. Invalid integers rejected.
pub fn isTimestampFresh(timestamp: []const u8, now: i64, max_drift: i64) bool {
    const ts = std.fmt.parseInt(i64, timestamp, 10) catch return false;
    if (ts <= 0) return false;
    if (ts > now + max_drift) return false;
    if (now > ts and now - ts > max_drift) return false;
    return true;
}

test {
    _ = @import("slack_signature_test.zig");
}
