//! `webhook_hmac` middleware (M18_002 §3.3).
//!
//! Verifies an RFC-style HMAC-SHA256 body signature using the same basestring
//! + headers the existing approval endpoint ships:
//!   - header `x-signature-timestamp`: unix seconds
//!   - header `x-signature`: `"v0=" ++ hex(HmacSha256(secret, "v0:<ts>:<body>"))`
//!   - max age 300s to prevent replay
//!   - constant-time hex compare
//!
//! Config carries the signing secret. The middleware never reads env vars —
//! hosts that loaded the secret from env/vault at boot pass the raw bytes in.
//! Host-supplied `now_ms_fn` lets tests pin the clock without relying on
//! wall-clock drift.
//!
//! Spec §Error Contracts lists "UZ-WH-010" for invalid HMAC, but that code
//! is already bound to Slack signature failures in `src/errors/`. We reuse
//! the approval-domain code that's been in production since M8.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

pub const SIGNATURE_MAX_AGE_SECONDS: i64 = 300;
pub const SIGNATURE_VERSION: []const u8 = "v0";

/// Milliseconds-since-epoch provider; swapped in tests.
pub const NowMsFn = *const fn () i64;

fn defaultNowMs() i64 {
    return std.time.milliTimestamp();
}

pub const WebhookHmac = struct {
    /// Raw signing secret (not base64-encoded). Host loads from env/vault.
    secret: []const u8,
    now_ms: NowMsFn = defaultNowMs,

    pub fn middleware(self: *WebhookHmac) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *WebhookHmac = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *WebhookHmac, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const timestamp = req.header("x-signature-timestamp") orelse {
            ctx.fail(errors.ERR_APPROVAL_INVALID_SIGNATURE, "Missing signature timestamp");
            return .short_circuit;
        };
        const provided_sig = req.header("x-signature") orelse {
            ctx.fail(errors.ERR_APPROVAL_INVALID_SIGNATURE, "Missing signature");
            return .short_circuit;
        };

        const ts = std.fmt.parseInt(i64, timestamp, 10) catch {
            ctx.fail(errors.ERR_APPROVAL_INVALID_SIGNATURE, "Invalid timestamp");
            return .short_circuit;
        };
        const now_s = @divTrunc(self.now_ms(), 1000);
        if (@abs(now_s - ts) > SIGNATURE_MAX_AGE_SECONDS) {
            ctx.fail(errors.ERR_APPROVAL_INVALID_SIGNATURE, "Timestamp too old");
            return .short_circuit;
        }

        const body = req.body() orelse "";

        var mac: [HmacSha256.mac_length]u8 = undefined;
        var h = HmacSha256.init(self.secret);
        h.update(SIGNATURE_VERSION);
        h.update(":");
        h.update(timestamp);
        h.update(":");
        h.update(body);
        h.final(&mac);

        // Expected: "v0=" ++ lowercase hex of the MAC.
        var expected_buf: [SIGNATURE_VERSION.len + 1 + HmacSha256.mac_length * 2]u8 = undefined;
        @memcpy(expected_buf[0..SIGNATURE_VERSION.len], SIGNATURE_VERSION);
        expected_buf[SIGNATURE_VERSION.len] = '=';
        const hex_chars = "0123456789abcdef";
        const hex_start = SIGNATURE_VERSION.len + 1;
        for (mac, 0..) |byte, i| {
            expected_buf[hex_start + i * 2] = hex_chars[byte >> 4];
            expected_buf[hex_start + i * 2 + 1] = hex_chars[byte & 0x0f];
        }
        const expected = expected_buf[0..];

        if (!constantTimeEqual(provided_sig, expected)) {
            ctx.fail(errors.ERR_APPROVAL_INVALID_SIGNATURE, "Invalid signature");
            return .short_circuit;
        }
        return .next;
    }
};

/// Constant-time equality over two byte slices. Length mismatch is folded in
/// *after* the XOR loop so we don't short-circuit on secret data.
fn constantTimeEqual(a: []const u8, b: []const u8) bool {
    const min_len = @min(a.len, b.len);
    var diff: u8 = 0;
    for (a[0..min_len], b[0..min_len]) |x, y| diff |= x ^ y;
    diff |= @as(u8, @intFromBool(a.len != b.len));
    return diff == 0;
}

test {
    _ = @import("webhook_hmac_test.zig");
}
