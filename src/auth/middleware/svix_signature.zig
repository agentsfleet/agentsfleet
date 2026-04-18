//! `svix_signature` middleware (M28_001 §5).
//!
//! Verifies Svix v1 multi-sig HMAC-SHA256 for Clerk (and any future Svix-
//! signed provider) webhooks. Route path `/v1/webhooks/svix/{zombie_id}`
//! selects this middleware statically; no per-request provider switch.
//!
//! Secret format: `whsec_<base64>`. The middleware strips the prefix and
//! base64-decodes the remainder into the raw HMAC key.
//!
//! Basestring: `{svix-id}.{svix-timestamp}.{body}` (dots, not colons).
//! `svix-signature` is space-delimited `v1,<base64_sig>` entries. The
//! middleware iterates and accepts if any entry matches — this is how
//! Clerk exposes key rotation. Comparison is constant-time.
//!
//! Live in `src/auth/` under the portability gate — imports only std,
//! httpz, and the `hmac_sig` named module.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const hs = @import("hmac_sig");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const SVIX_ID_HEADER: []const u8 = "svix-id";
pub const SVIX_TS_HEADER: []const u8 = "svix-timestamp";
pub const SVIX_SIG_HEADER: []const u8 = "svix-signature";
pub const SVIX_SECRET_PREFIX: []const u8 = "whsec_";
pub const SVIX_SIG_VERSION: []const u8 = "v1";
pub const SVIX_MAX_DRIFT_SECONDS: i64 = 300;

/// Max decoded secret length. Svix keys are 24 bytes; buffer is generous.
const MAX_SECRET_LEN: usize = 64;

pub const NowSecondsFn = *const fn () i64;

fn defaultNowSeconds() i64 {
    return std.time.timestamp();
}

const log = std.log.scoped(.svix);

/// Owned lookup result. `secret` is the `whsec_<base64>` string loaded from
/// vault for the target zombie — the middleware handles prefix stripping
/// and base64 decoding internally.
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
            return verify(ctx, req, secret_raw, self.now_seconds());
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

fn verify(ctx: *AuthCtx, req: *httpz.Request, secret_raw: []const u8, now: i64) chain.Outcome {
    if (!std.mem.startsWith(u8, secret_raw, SVIX_SECRET_PREFIX)) {
        log.warn("svix secret missing whsec_ prefix (misconfig) req_id={s}", .{ctx.req_id});
        return failSig(ctx);
    }
    var secret_buf: [MAX_SECRET_LEN]u8 = undefined;
    const secret = decodeBase64(&secret_buf, secret_raw[SVIX_SECRET_PREFIX.len..]) orelse {
        return failSig(ctx);
    };
    // Defense-in-depth: empty key → HMAC is deterministic and attacker-computable.
    // Parser rejects empty secret_ref upstream; this guards against vault misconfig
    // that stored a literal "whsec_" (prefix only) or produced an empty decode.
    if (secret.len == 0) return failSig(ctx);

    const svix_id = req.header(SVIX_ID_HEADER) orelse return failSig(ctx);
    const svix_ts = req.header(SVIX_TS_HEADER) orelse return failSig(ctx);
    const svix_sigs = req.header(SVIX_SIG_HEADER) orelse return failSig(ctx);

    if (!isTimestampFresh(svix_ts, now, SVIX_MAX_DRIFT_SECONDS)) return failStale(ctx);

    const body = req.body() orelse "";
    const expected_mac = hs.computeMac(secret, &.{ svix_id, ".", svix_ts, ".", body });

    if (anySignatureMatches(svix_sigs, &expected_mac)) return .next;
    return failSig(ctx);
}

/// Iterate space-delimited `v1,<base64>` entries; return true on first
/// constant-time match against `expected_mac`. Unknown versions skipped.
fn anySignatureMatches(svix_sigs: []const u8, expected_mac: *const [hs.MAC_LEN]u8) bool {
    var it = std.mem.splitScalar(u8, svix_sigs, ' ');
    while (it.next()) |entry| {
        if (entry.len == 0) continue;
        const comma = std.mem.indexOfScalar(u8, entry, ',') orelse continue;
        if (!std.mem.eql(u8, entry[0..comma], SVIX_SIG_VERSION)) continue;
        var sig_buf: [hs.MAC_LEN]u8 = undefined;
        const decoded = decodeBase64(&sig_buf, entry[comma + 1 ..]) orelse continue;
        if (decoded.len != hs.MAC_LEN) continue;
        if (hs.constantTimeEql(expected_mac, &sig_buf)) return true;
    }
    return false;
}

/// Decode standard base64. Svix signatures use padded standard base64; the
/// secret body is also standard base64 (padded in practice). Falls back to
/// no-pad on padding-related errors so either form works.
fn decodeBase64(dest: []u8, src: []const u8) ?[]u8 {
    if (tryDecode(&std.base64.standard.Decoder, dest, src)) |n| return dest[0..n];
    if (tryDecode(&std.base64.standard_no_pad.Decoder, dest, src)) |n| return dest[0..n];
    return null;
}

fn tryDecode(decoder: *const std.base64.Base64Decoder, dest: []u8, src: []const u8) ?usize {
    const size = decoder.calcSizeForSlice(src) catch return null;
    if (size > dest.len) return null;
    decoder.decode(dest[0..size], src) catch return null;
    return size;
}

/// Freshness check over unix-seconds timestamp string. Rejects pre-signed
/// requests; caller-injectable `now` for deterministic tests.
pub fn isTimestampFresh(timestamp: []const u8, now: i64, max_drift: i64) bool {
    const ts = std.fmt.parseInt(i64, timestamp, 10) catch return false;
    if (ts <= 0) return false;
    if (ts > now + max_drift) return false;
    if (now > ts and now - ts > max_drift) return false;
    return true;
}

test {
    _ = @import("svix_signature_test.zig");
}
