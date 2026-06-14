//! Shared helpers for the CLI device-flow session handlers.
//!
//! Sibling to `sessions.zig`. Holds the request-scratch derivation
//! (client-IP attribution + user-agent capture + redact-buffer),
//! the verify-outcome response shaping, and the store-error → HTTP
//! mapping. Keeps `sessions.zig` focused on the entry-point handlers
//! and within the file-length gate.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const error_codes = @import("../../../errors/error_registry.zig");
const hx_mod = @import("../hx.zig");
const audit = @import("../../../auth/audit.zig");
const audit_events = @import("../../../auth/audit_events.zig");
const trusted_ip = @import("../../../auth/middleware/trusted_client_ip.zig");
const session_store = @import("../../../session/session_store_redis.zig");

const log = logging.scoped(.auth);
const Sha256 = std.crypto.hash.sha2.Sha256;

pub const HDR_XFF = "X-Forwarded-For";
pub const HDR_FLY = "Fly-Client-IP";
pub const HDR_USER_AGENT = "user-agent";
pub const S_USER_AGENT_UNKNOWN = "unknown";

pub const REDACT_BUF_LEN: usize = 32;
pub const IP_BUF_LEN: usize = 64;
pub const FINGERPRINT_HEX_LEN: usize = 64;

/// Per-request scratch — owns the formatted TCP-peer string + the
/// derived-IP struct + the user-agent slice for the request lifetime.
/// `ip_buf` carries the formatted TCP-peer bytes that `derived.ip` may
/// alias when the request has no `X-Forwarded-For` / `Fly-Client-IP`
/// header; both must share the request lifetime.
pub const RequestScratch = struct {
    ip_buf: [IP_BUF_LEN]u8,
    derived: trusted_ip.DerivedClientIp,
    user_agent: []const u8,
};

pub fn buildScratch(scratch: *RequestScratch, req: *httpz.Request) void {
    const peer = formatIpOnly(&scratch.ip_buf, req.address);
    scratch.derived = trusted_ip.deriveClientIp(peer, req.header(HDR_XFF), req.header(HDR_FLY));
    scratch.user_agent = req.header(HDR_USER_AGENT) orelse S_USER_AGENT_UNKNOWN;
}

/// Io.net.IpAddress.format yields `"ip:port"` or `"[ipv6]:port"`. We
/// want just the host part for both the audit chain and rate-limit
/// buckets — strip the port (and brackets when present).
pub fn formatIpOnly(buf: []u8, addr: std.Io.net.IpAddress) []const u8 {
    const formatted = std.fmt.bufPrint(buf, "{f}", .{addr}) catch return "";
    if (formatted.len > 0 and formatted[0] == '[') {
        const rb = std.mem.indexOfScalar(u8, formatted, ']') orelse return "";
        return formatted[1..rb];
    }
    const colon = std.mem.lastIndexOfScalar(u8, formatted, ':') orelse return formatted;
    return formatted[0..colon];
}

pub fn computeFingerprintHex(
    out: *[FINGERPRINT_HEX_LEN]u8,
    ip: []const u8,
    ua: []const u8,
    sid: []const u8,
) []const u8 {
    // 0x00 separator between fields. Without it, `update(ip) || update(ua) ||
    // update(sid)` is byte-equal to `update(ip||ua||sid)`, so a user-controlled
    // UA can be crafted such that `ip2 || ua2 == ip1 || ua1` and two distinct
    // requests collide on the same fingerprint within the 60s replay window.
    // 0x00 isn't a valid byte in IPv4/IPv6 textual form or any HTTP header
    // value (RFC 7230 section 3.2.6), so the separator can't appear inside
    // `ip` or `ua` and cannot be smuggled into the hash from either field.
    var h = Sha256.init(.{});
    h.update(ip);
    h.update(&.{0x00});
    h.update(ua);
    h.update(&.{0x00});
    h.update(sid);
    var digest: [Sha256.digest_length]u8 = undefined;
    h.final(&digest);
    const hex = std.fmt.bytesToHex(digest, .lower);
    @memcpy(out, &hex);
    return out;
}

pub fn redactSid(buf: *[REDACT_BUF_LEN]u8, sid: []const u8) []const u8 {
    return audit.redactSessionIdInto(buf, sid);
}

// ── Verify outcome response shaping ──────────────────────────────────────

pub fn dispatchVerifyOutcome(
    hx: hx_mod.Hx,
    outcome: session_store.VerifyOutcome,
    session_id: []const u8,
    fingerprint: []const u8,
    scratch: RequestScratch,
) void {
    switch (outcome) {
        .success => |p| onVerifySuccess(hx, p, session_id, fingerprint, scratch),
        .replay => |p| onVerifyReplay(hx, p, session_id, fingerprint),
        .invalid_code => |attempts| onVerifyInvalid(hx, attempts, session_id, scratch),
        .rate_limited => onVerifyRateLimited(hx, session_id, scratch),
        .not_approved => onVerifyNotApproved(hx, session_id, scratch),
        .aborted => |reason| hx.fail(error_codes.ERR_SESSION_ABORTED, reason),
        .consumed => hx.fail(error_codes.ERR_SESSION_CONSUMED, "Session already consumed"),
        .expired => hx.fail(error_codes.ERR_SESSION_EXPIRED, "Session expired"),
        .missing => hx.fail(error_codes.ERR_SESSION_NOT_FOUND, "Session not found"),
    }
}

fn onVerifySuccess(
    hx: hx_mod.Hx,
    payload: session_store.VerifyPayload,
    session_id: []const u8,
    fingerprint: []const u8,
    scratch: RequestScratch,
) void {
    // `token_name` omitted from the verified event — capturing it would
    // require an extra Redis read post-consume, and the session_id_hash
    // already correlates this event with the prior `created`/`approved`
    // events that carry the label.
    audit_events.emitSessionVerified(
        hx.ctx.audit_ctx,
        session_id,
        "",
        0,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );
    audit_events.emitSessionConsumed(hx.ctx.audit_ctx, session_id, fingerprint, hx.req_id);
    hx.ok(.ok, .{
        .dashboard_public_key = payload.dashboard_public_key,
        .ciphertext = payload.ciphertext,
        .nonce = payload.nonce,
    });
}

fn onVerifyReplay(
    hx: hx_mod.Hx,
    payload: session_store.VerifyPayload,
    session_id: []const u8,
    fingerprint: []const u8,
) void {
    audit_events.emitSessionConsumedReplay(hx.ctx.audit_ctx, session_id, fingerprint, 0, hx.req_id);
    hx.ok(.ok, .{
        .dashboard_public_key = payload.dashboard_public_key,
        .ciphertext = payload.ciphertext,
        .nonce = payload.nonce,
    });
}

fn onVerifyInvalid(hx: hx_mod.Hx, attempts: u8, session_id: []const u8, scratch: RequestScratch) void {
    audit_events.emitSessionVerifyFailed(
        hx.ctx.audit_ctx,
        session_id,
        attempts,
        audit_events.REASON_INVALID_CODE,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );
    hx.fail(error_codes.ERR_VERIFICATION_FAILED, "Verification code did not match");
}

// The verify attempt that just tripped MAX_VERIFY_ATTEMPTS. The Lua already
// flipped the session to aborted; emit the final verify_failed + the one-shot
// abort audit here (on the edge), then return the terminal 410 — not the
// retryable ERR_VERIFICATION_FAILED — so the CLI stops prompting and the user
// restarts login instead of burning the remaining client-side retry.
fn onVerifyRateLimited(hx: hx_mod.Hx, session_id: []const u8, scratch: RequestScratch) void {
    audit_events.emitSessionVerifyFailed(
        hx.ctx.audit_ctx,
        session_id,
        session_store.MAX_VERIFY_ATTEMPTS,
        audit_events.REASON_INVALID_CODE,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );
    audit_events.emitSessionAborted(
        hx.ctx.audit_ctx,
        session_id,
        audit_events.REASON_RATE_LIMIT_EXCEEDED,
        null,
        scratch.derived,
        hx.req_id,
    );
    hx.fail(error_codes.ERR_SESSION_ABORTED, "Too many incorrect attempts — session aborted");
}

// A /verify against a still-pending (un-approved) session. The CLI polls to
// verification_pending before prompting, so this is an out-of-order or probing
// attempt worth a verify_failed record. attempts=0: no code was checked — the
// session never reached the code gate.
fn onVerifyNotApproved(hx: hx_mod.Hx, session_id: []const u8, scratch: RequestScratch) void {
    audit_events.emitSessionVerifyFailed(
        hx.ctx.audit_ctx,
        session_id,
        0,
        audit_events.REASON_NOT_APPROVED,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );
    hx.fail(error_codes.ERR_SESSION_NOT_APPROVED, "Session not approved yet");
}

// ── Store-error → HTTP mapping ───────────────────────────────────────────

pub fn failFromStoreError(hx: hx_mod.Hx, err: anyerror, session_id: ?[]const u8) void {
    var rbuf: [REDACT_BUF_LEN]u8 = undefined;
    const redacted = if (session_id) |sid| redactSid(&rbuf, sid) else "";
    log.warn("auth_session_store_error", .{ .err = @errorName(err), .session_id = redacted, .req_id = hx.req_id });

    const code: []const u8 = switch (err) {
        session_store.Error.InvalidPublicKey => error_codes.ERR_INVALID_PUBLIC_KEY,
        session_store.Error.InvalidTokenName => error_codes.ERR_INVALID_TOKEN_NAME,
        session_store.Error.InvalidCipherText => error_codes.ERR_INVALID_CIPHERTEXT,
        session_store.Error.InvalidNonce => error_codes.ERR_INVALID_NONCE,
        session_store.Error.InvalidVerificationCode => error_codes.ERR_INVALID_VERIFICATION_CODE,
        session_store.Error.AlreadyApproved => error_codes.ERR_SESSION_ALREADY_APPROVED,
        session_store.Error.SessionMissing => error_codes.ERR_SESSION_NOT_FOUND,
        session_store.Error.NotOwner => error_codes.ERR_FORBIDDEN,
        session_store.Error.SessionConsumed => error_codes.ERR_SESSION_CONSUMED,
        session_store.Error.SessionAborted => error_codes.ERR_SESSION_ABORTED,
        else => error_codes.ERR_INTERNAL_OPERATION_FAILED,
    };
    hx.fail(code, @errorName(err));
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "formatIpOnly strips port from an IPv4 address" {
    var buf: [IP_BUF_LEN]u8 = undefined;
    const addr = try std.Io.net.IpAddress.parseIp4("203.0.113.7", 8443);
    try testing.expectEqualStrings("203.0.113.7", formatIpOnly(&buf, addr));
}

test "formatIpOnly strips brackets + port from an IPv6 address" {
    var buf: [IP_BUF_LEN]u8 = undefined;
    const addr = try std.Io.net.IpAddress.parseIp6("::1", 8443);
    try testing.expectEqualStrings("::1", formatIpOnly(&buf, addr));
}

test "computeFingerprintHex is deterministic per (ip,ua,sid) triple" {
    var a: [FINGERPRINT_HEX_LEN]u8 = undefined;
    var b: [FINGERPRINT_HEX_LEN]u8 = undefined;
    _ = computeFingerprintHex(&a, "1.2.3.4", "ua/1", "sid-1");
    _ = computeFingerprintHex(&b, "1.2.3.4", "ua/1", "sid-1");
    try testing.expectEqualSlices(u8, &a, &b);
}

test "computeFingerprintHex differs when any input differs" {
    var a: [FINGERPRINT_HEX_LEN]u8 = undefined;
    var b: [FINGERPRINT_HEX_LEN]u8 = undefined;
    var c: [FINGERPRINT_HEX_LEN]u8 = undefined;
    var d: [FINGERPRINT_HEX_LEN]u8 = undefined;
    _ = computeFingerprintHex(&a, "1.2.3.4", "ua/1", "sid-1");
    _ = computeFingerprintHex(&b, "1.2.3.5", "ua/1", "sid-1");
    _ = computeFingerprintHex(&c, "1.2.3.4", "ua/2", "sid-1");
    _ = computeFingerprintHex(&d, "1.2.3.4", "ua/1", "sid-2");
    try testing.expect(!std.mem.eql(u8, &a, &b));
    try testing.expect(!std.mem.eql(u8, &a, &c));
    try testing.expect(!std.mem.eql(u8, &a, &d));
}

test "computeFingerprintHex resists ip||ua boundary collision" {
    // Without the 0x00 field separator, ("12","3") and ("1","23") would
    // both hash `12||3 == 1||23` and collide. The separator makes the
    // field boundary unambiguous, so the two distinct (ip,ua) pairs must
    // produce distinct fingerprints for the same sid.
    var a: [FINGERPRINT_HEX_LEN]u8 = undefined;
    var b: [FINGERPRINT_HEX_LEN]u8 = undefined;
    _ = computeFingerprintHex(&a, "12", "3", "sid-1");
    _ = computeFingerprintHex(&b, "1", "23", "sid-1");
    try testing.expect(!std.mem.eql(u8, &a, &b));
}
