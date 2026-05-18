//! CLI device-flow auth session handlers — five endpoints.
//!
//!   POST    /v1/auth/sessions                — create        (no auth)
//!   GET     /v1/auth/sessions/{id}           — poll          (no auth)
//!   PATCH   /v1/auth/sessions/{id}/approve   — dashboard     (Clerk JWT)
//!   POST    /v1/auth/sessions/{id}/verify    — submit code   (no auth)
//!   DELETE  /v1/auth/sessions/{id}           — explicit cancel (Clerk JWT)
//!   DELETE  /v1/auth/sessions/all            — abort all     (Clerk JWT)
//!
//! The plaintext PATCH /v1/auth/sessions/{id} shape that the prior
//! in-memory store served never shipped to production (Captain Q3). The
//! router returns 404 for that path now; this handler implements only the
//! new state-machine.
//!
//! Shared scratch + verify-outcome dispatch + store-error mapping live in
//! `session_helpers.zig`. All `.auth` info/warn/error log emits go through
//! `helpers.redactSid` per Invariant 16; the `.auth_audit` scope (via
//! `audit_events.emit*`) is the only surface that sees the raw session_id
//! (and only via its hashed + prefixed forms).

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const error_codes = @import("../../../errors/error_registry.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const audit_events = @import("../../../auth/audit_events.zig");
const helpers = @import("session_helpers.zig");

const log = logging.scoped(.auth);

pub const Context = common.Context;

// ── POST /v1/auth/sessions ───────────────────────────────────────────────

pub fn innerCreateAuthSession(hx: hx_mod.Hx, req: *httpz.Request) void {
    // SAFETY: every field is populated by `helpers.buildScratch` on the next line before any read.
    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, req);

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const Body = struct { public_key: []const u8, token_name: []const u8 };
    const parsed = std.json.parseFromSlice(Body, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing public_key/token_name");
        return;
    };
    defer parsed.deinit();

    const session_id = hx.ctx.auth_sessions.create(parsed.value.public_key, parsed.value.token_name) catch |err| {
        return helpers.failFromStoreError(hx, err, null);
    };

    audit_events.emitSessionCreated(
        hx.ctx.audit_ctx,
        session_id,
        parsed.value.token_name,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );

    var rbuf: [helpers.REDACT_BUF_LEN]u8 = undefined;
    log.info("auth_session_created", .{ .session_id = helpers.redactSid(&rbuf, session_id), .req_id = hx.req_id });
    hx.ok(.created, .{ .session_id = session_id, .request_id = hx.req_id });
}

// ── GET /v1/auth/sessions/{id} ───────────────────────────────────────────

pub fn innerPollAuthSession(hx: hx_mod.Hx, session_id: []const u8) void {
    var parsed = hx.ctx.auth_sessions.get(session_id) catch {
        common.internalOperationError(hx.res, "Session lookup failed", hx.req_id);
        return;
    };
    if (parsed == null) {
        hx.fail(error_codes.ERR_SESSION_NOT_FOUND, "Session not found");
        return;
    }
    defer parsed.?.deinit();
    const s = parsed.?.value;
    switch (s.status) {
        .pending, .verification_pending => hx.ok(.ok, .{
            .status = @tagName(s.status),
            .cli_public_key = s.cli_public_key,
            .token_name = s.token_name,
            .expires_at_ms = s.expires_at_ms,
        }),
        .consumed => hx.fail(error_codes.ERR_SESSION_CONSUMED, "Session already consumed"),
        .expired => hx.fail(error_codes.ERR_SESSION_EXPIRED, "Session expired"),
        .aborted => hx.fail(error_codes.ERR_SESSION_ABORTED, s.aborted_reason orelse "aborted"),
    }
}

// ── PATCH /v1/auth/sessions/{id}/approve ─────────────────────────────────

pub fn innerApproveAuthSession(hx: hx_mod.Hx, req: *httpz.Request, session_id: []const u8) void {
    const clerk_user_id = hx.principal.user_id orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Clerk user context missing");
        return;
    };
    // SAFETY: every field is populated by `helpers.buildScratch` on the next line before any read.
    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, req);

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const ApproveBody = struct {
        dashboard_public_key: []const u8,
        ciphertext: []const u8,
        nonce: []const u8,
        verification_code: []const u8,
    };
    const parsed = std.json.parseFromSlice(ApproveBody, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed approve payload");
        return;
    };
    defer parsed.deinit();

    hx.ctx.auth_sessions.approve(
        session_id,
        parsed.value.dashboard_public_key,
        parsed.value.ciphertext,
        parsed.value.nonce,
        parsed.value.verification_code,
        clerk_user_id,
    ) catch |err| return helpers.failFromStoreError(hx, err, session_id);

    finishApprove(hx, session_id, clerk_user_id, scratch);
}

fn finishApprove(hx: hx_mod.Hx, session_id: []const u8, clerk_user_id: []const u8, scratch: helpers.RequestScratch) void {
    var maybe_parsed = hx.ctx.auth_sessions.get(session_id) catch null;
    const token_name = if (maybe_parsed) |p| p.value.token_name else "";
    defer if (maybe_parsed) |*p| p.deinit();

    audit_events.emitSessionApproved(
        hx.ctx.audit_ctx,
        session_id,
        clerk_user_id,
        token_name,
        scratch.derived,
        scratch.user_agent,
        hx.req_id,
    );
    var rbuf: [helpers.REDACT_BUF_LEN]u8 = undefined;
    log.info("auth_session_approved", .{ .session_id = helpers.redactSid(&rbuf, session_id), .req_id = hx.req_id });
    hx.ok(.ok, .{ .request_id = hx.req_id });
}

// ── POST /v1/auth/sessions/{id}/verify ───────────────────────────────────

pub fn innerVerifyAuthSession(hx: hx_mod.Hx, req: *httpz.Request, session_id: []const u8) void {
    // SAFETY: every field is populated by `helpers.buildScratch` on the next line before any read.
    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, req);

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const Body = struct { verification_code: []const u8 };
    const parsed = std.json.parseFromSlice(Body, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed verify payload");
        return;
    };
    defer parsed.deinit();

    var fp_buf: [helpers.FINGERPRINT_HEX_LEN]u8 = undefined;
    const fingerprint = helpers.computeFingerprintHex(&fp_buf, scratch.derived.ip, scratch.user_agent, session_id);

    const outcome = hx.ctx.auth_sessions.verifyAndConsume(session_id, parsed.value.verification_code, fingerprint) catch |err| {
        return helpers.failFromStoreError(hx, err, session_id);
    };
    helpers.dispatchVerifyOutcome(hx, outcome, session_id, fingerprint, scratch);
}

// ── DELETE /v1/auth/sessions/{id} ────────────────────────────────────────

pub fn innerDeleteAuthSession(hx: hx_mod.Hx, req: *httpz.Request, session_id: []const u8) void {
    const clerk_user_id = hx.principal.user_id orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Clerk user context missing");
        return;
    };
    // SAFETY: every field is populated by `helpers.buildScratch` on the next line before any read.
    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, req);

    hx.ctx.auth_sessions.delete(session_id, clerk_user_id) catch |err| {
        return helpers.failFromStoreError(hx, err, session_id);
    };

    audit_events.emitSessionAborted(
        hx.ctx.audit_ctx,
        session_id,
        audit_events.REASON_EXPLICIT_CANCEL,
        clerk_user_id,
        scratch.derived,
        hx.req_id,
    );
    hx.noContent();
}

// ── DELETE /v1/auth/sessions/all ─────────────────────────────────────────

pub fn innerDeleteAllAuthSessions(hx: hx_mod.Hx) void {
    const clerk_user_id = hx.principal.user_id orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Clerk user context missing");
        return;
    };
    const count = hx.ctx.auth_sessions.deleteAllForUser(clerk_user_id) catch {
        common.internalOperationError(hx.res, "Bulk session abort failed", hx.req_id);
        return;
    };
    log.info("auth_sessions_bulk_aborted", .{ .clerk_user_id = clerk_user_id, .count = count, .req_id = hx.req_id });
    hx.ok(.ok, .{ .aborted_count = count });
}
