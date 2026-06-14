//! Per-event audit emitters for the CLI device-flow auth surface.
//!
//! One function per audit event in the spec's event-schema table.
//! Each emitter consumes a typed payload + the boot-loaded
//! `AUDIT_LOG_PEPPER`, pseudonymizes the raw `session_id` via
//! `audit.sessionIdHashHex` + `audit.sessionIdPrefix`, then writes a
//! single structured record via `logging.scoped(.auth_audit)`.
//!
//! The `.auth_audit` scope is the dedicated sink — deploy-side log
//! routing fans it to a restricted destination (separate Loki tenant,
//! separate syslog facility, security-team S3 bucket) per the spec's
//! restricted-routing requirement. This module never sees the raw
//! session_id leaving the process — the HMAC layer is the only
//! exposure shape downstream consumers ever observe.
//!
//! Stays portable (no httpz / handler imports). Handler-side wiring
//! reads request headers + composes the `DerivedClientIp` via
//! `middleware/trusted_client_ip.zig`, then passes the result here.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const audit = @import("audit.zig");
const trusted_ip = @import("middleware/trusted_client_ip.zig");
const AUDIT_RECORD_EVENT = "audit_record";

const audit_log = logging.scoped(.auth_audit);

// ── Event name constants (RULE UFS) ──────────────────────────────────────

pub const EV_SESSION_CREATED: []const u8 = "auth.session.created";
pub const EV_SESSION_APPROVED: []const u8 = "auth.session.approved";
pub const EV_SESSION_VERIFY_FAILED: []const u8 = "auth.session.verify_failed";
pub const EV_SESSION_VERIFIED: []const u8 = "auth.session.verified";
pub const EV_SESSION_CONSUMED: []const u8 = "auth.session.consumed";
pub const EV_SESSION_CONSUMED_REPLAY: []const u8 = "auth.session.consumed_replay";
pub const EV_SESSION_ABORTED: []const u8 = "auth.session.aborted";

// Reason enum string constants for aborted / verify_failed.
// REASON_RATE_LIMIT_EXCEEDED stays — auth.session.aborted fires it when the
// L3 5-attempt cap in verifyAndConsume Lua trips. The verify_failed
// REASON_RATE_LIMITED was for an in-app /verify per-IP middleware path that
// is no longer authored (Captain decision Q10) — removed.
pub const REASON_INVALID_CODE: []const u8 = "invalid_code";
pub const REASON_NOT_APPROVED: []const u8 = "not_approved";
pub const REASON_RATE_LIMIT_EXCEEDED: []const u8 = "rate_limit_exceeded";
pub const REASON_EXPLICIT_CANCEL: []const u8 = "explicit_cancel";

/// Shared boot-loaded state. Constructed once at serve init and passed
/// onto the request context so per-request emit calls don't re-read env.
pub const AuditCtx = struct {
    audit_log_pepper: []const u8,

    pub fn init(audit_log_pepper: []const u8) AuditCtx {
        return .{ .audit_log_pepper = audit_log_pepper };
    }
};

// ── Emitters ─────────────────────────────────────────────────────────────

pub fn emitSessionCreated(
    ctx: AuditCtx,
    session_id: []const u8,
    token_name: []const u8,
    derived_ip: trusted_ip.DerivedClientIp,
    user_agent: []const u8,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_CREATED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .token_name = token_name,
        .ip = derived_ip.ip,
        .xff = derived_ip.xff_raw,
        .fly_client_ip = derived_ip.fly_client_ip_raw,
        .client_ip_source = @tagName(derived_ip.source),
        .client_ip_divergent = derived_ip.divergent,
        .user_agent = user_agent,
        .request_id = request_id,
    });
}

pub fn emitSessionApproved(
    ctx: AuditCtx,
    session_id: []const u8,
    clerk_user_id: []const u8,
    token_name: []const u8,
    derived_ip: trusted_ip.DerivedClientIp,
    user_agent: []const u8,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_APPROVED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .clerk_user_id = clerk_user_id,
        .token_name = token_name,
        .ip = derived_ip.ip,
        .xff = derived_ip.xff_raw,
        .fly_client_ip = derived_ip.fly_client_ip_raw,
        .client_ip_source = @tagName(derived_ip.source),
        .client_ip_divergent = derived_ip.divergent,
        .user_agent = user_agent,
        .request_id = request_id,
    });
}

pub fn emitSessionVerifyFailed(
    ctx: AuditCtx,
    session_id: []const u8,
    attempt: u8,
    reason: []const u8,
    derived_ip: trusted_ip.DerivedClientIp,
    user_agent: []const u8,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_VERIFY_FAILED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .attempt = attempt,
        .reason = reason,
        .ip = derived_ip.ip,
        .xff = derived_ip.xff_raw,
        .fly_client_ip = derived_ip.fly_client_ip_raw,
        .client_ip_source = @tagName(derived_ip.source),
        .client_ip_divergent = derived_ip.divergent,
        .user_agent = user_agent,
        .request_id = request_id,
    });
}

pub fn emitSessionVerified(
    ctx: AuditCtx,
    session_id: []const u8,
    token_name: []const u8,
    attempt: u8,
    derived_ip: trusted_ip.DerivedClientIp,
    user_agent: []const u8,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_VERIFIED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .token_name = token_name,
        .attempt = attempt,
        .ip = derived_ip.ip,
        .xff = derived_ip.xff_raw,
        .fly_client_ip = derived_ip.fly_client_ip_raw,
        .client_ip_source = @tagName(derived_ip.source),
        .client_ip_divergent = derived_ip.divergent,
        .user_agent = user_agent,
        .request_id = request_id,
    });
}

pub fn emitSessionConsumed(
    ctx: AuditCtx,
    session_id: []const u8,
    consumed_client_fingerprint_hex: []const u8,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_CONSUMED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .consumed_client_fingerprint = consumed_client_fingerprint_hex,
        .request_id = request_id,
    });
}

pub fn emitSessionConsumedReplay(
    ctx: AuditCtx,
    session_id: []const u8,
    consumed_client_fingerprint_hex: []const u8,
    replay_within_ms: i64,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_CONSUMED_REPLAY,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .consumed_client_fingerprint = consumed_client_fingerprint_hex,
        .replay_within_ms = replay_within_ms,
        .request_id = request_id,
    });
}

pub fn emitSessionAborted(
    ctx: AuditCtx,
    session_id: []const u8,
    reason: []const u8,
    clerk_user_id: ?[]const u8,
    derived_ip: trusted_ip.DerivedClientIp,
    request_id: []const u8,
) void {
    var hash_buf: [audit.SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const sid_hash = audit.sessionIdHashHex(&hash_buf, ctx.audit_log_pepper, session_id);
    audit_log.info(AUDIT_RECORD_EVENT, .{
        .event = EV_SESSION_ABORTED,
        .ts_ms = clock.nowMillis(),
        .session_id_hash = sid_hash,
        .session_id_prefix = audit.sessionIdPrefix(session_id),
        .reason = reason,
        .clerk_user_id = clerk_user_id,
        .ip = derived_ip.ip,
        .xff = derived_ip.xff_raw,
        .fly_client_ip = derived_ip.fly_client_ip_raw,
        .client_ip_source = @tagName(derived_ip.source),
        .client_ip_divergent = derived_ip.divergent,
        .request_id = request_id,
    });
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "event name constants are stable and namespaced" {
    try testing.expectEqualStrings("auth.session.created", EV_SESSION_CREATED);
    try testing.expectEqualStrings("auth.session.approved", EV_SESSION_APPROVED);
    try testing.expectEqualStrings("auth.session.verify_failed", EV_SESSION_VERIFY_FAILED);
    try testing.expectEqualStrings("auth.session.verified", EV_SESSION_VERIFIED);
    try testing.expectEqualStrings("auth.session.consumed", EV_SESSION_CONSUMED);
    try testing.expectEqualStrings("auth.session.consumed_replay", EV_SESSION_CONSUMED_REPLAY);
    try testing.expectEqualStrings("auth.session.aborted", EV_SESSION_ABORTED);
}

test "reason constants match the audit-event schema" {
    try testing.expectEqualStrings("invalid_code", REASON_INVALID_CODE);
    try testing.expectEqualStrings("not_approved", REASON_NOT_APPROVED);
    try testing.expectEqualStrings("rate_limit_exceeded", REASON_RATE_LIMIT_EXCEEDED);
    try testing.expectEqualStrings("explicit_cancel", REASON_EXPLICIT_CANCEL);
}

test "AuditCtx.init stores the pepper for later emit calls" {
    const ctx = AuditCtx.init("test-pepper-bytes-32-len--padded");
    try testing.expectEqualStrings("test-pepper-bytes-32-len--padded", ctx.audit_log_pepper);
}

test "emitSessionCreated runs without panic on representative input" {
    // Smoke: validates the type signature + pseudonymization integration.
    // Output goes to the project's std.log scope; verifying captured bytes
    // is a Slice 3 integration-test concern (test_audit_event_shape_*).
    const ctx = AuditCtx.init("test-pepper-bytes-32-len--padded");
    const derived = trusted_ip.deriveClientIp("203.0.113.7", null, null);
    emitSessionCreated(
        ctx,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40",
        "macos-cli",
        derived,
        "Mozilla/5.0",
        "req_abc123",
    );
}

test "emitSessionConsumed omits IP fields per the consume-event schema" {
    // Consume event ships with fingerprint only — no `ip` / `xff` etc.
    // Pinned because adding IP fields would create cross-session
    // correlation pressure the spec deliberately avoids on the consume
    // record (the fingerprint is already an IP-derived primitive).
    const ctx = AuditCtx.init("test-pepper-bytes-32-len--padded");
    emitSessionConsumed(
        ctx,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40",
        "abcd" ** 16,
        "req_abc",
    );
}

test "emitSessionVerifyFailed accepts the not_approved reason" {
    // Wiring smoke for the verify .not_approved arm: it now routes through
    // emitSessionVerifyFailed with REASON_NOT_APPROVED and attempts=0 (no code
    // was checked). Captured-bytes shape stays a Slice 3 integration concern.
    const ctx = AuditCtx.init("test-pepper-bytes-32-len--padded");
    const derived = trusted_ip.deriveClientIp("203.0.113.7", null, null);
    emitSessionVerifyFailed(
        ctx,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40",
        0,
        REASON_NOT_APPROVED,
        derived,
        "Mozilla/5.0",
        "req_abc123",
    );
}
