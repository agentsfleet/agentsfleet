//! Response-shaping proofs for the CLI device-flow session helpers.
//!
//! Two surfaces meet an unauthenticated caller here, so both are pinned arm by
//! arm rather than sampled:
//!
//!   - `failFromStoreError` decides what a login attempt learns when the store
//!     refuses it. Three of its inputs (Redis faults, allocation failure) are
//!     deliberately unclassified and must answer with the generic internal code
//!     — a raw Zig error tag reaching this caller is an information leak, and
//!     the arm that prevents it is one `else` away from being widened by
//!     accident.
//!   - `dispatchVerifyOutcome` maps the store's verdict onto the response the
//!     CLI branches on. `rate_limited` in particular must answer terminal, not
//!     retryable, or the CLI burns its remaining prompts against a session the
//!     Lua already aborted.
//!
//! The pure helpers (`formatIpOnly`, `computeFingerprintHex`) keep their tests
//! next to the code in `session_helpers.zig`; these need a response and a
//! context, so they live here.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const helpers = @import("session_helpers.zig");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const audit_events = @import("../../../auth/audit_events.zig");
const trusted_ip = @import("../../../auth/middleware/trusted_client_ip.zig");
const session_store = @import("../../../session/session_store_redis.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-session-1";
const SESSION_ID = "01932b7c-0000-7000-8000-000000000001";
const TEST_PEPPER = "0" ** 64;
const K_ERROR_CODE = "error_code";
const K_DETAIL = "detail";
const XFF_CLIENT = "198.51.100.9";

/// Only `audit_ctx` is populated: every path under test either ignores the
/// context entirely or reaches it solely to hash a session id for the audit
/// line. A path that grows a second read crashes here rather than passing
/// against a fixture that quietly grew the dependency.
fn buildCtx() common.Context {
    // SAFETY: see above — the fields left undefined are unread on these paths.
    var ctx: common.Context = undefined;
    ctx.audit_ctx = audit_events.AuditCtx.init(TEST_PEPPER);
    return ctx;
}

fn buildHx(res: *httpz.Response, ctx: *common.Context) Hx {
    return Hx{
        .alloc = testing.allocator,
        // SAFETY: none of these helpers read the principal — the CLI login flow
        // is unauthenticated by construction.
        .principal = undefined,
        .req_id = REQ_ID,
        .ctx = ctx,
        .res = res,
    };
}

fn buildScratchFixture() helpers.RequestScratch {
    return .{
        // SAFETY: `derived.ip` below points at a literal, not into this buffer,
        // so the bytes are never read on these paths.
        .ip_buf = undefined,
        .derived = trusted_ip.DerivedClientIp{
            .ip = XFF_CLIENT,
            .source = .xff,
            .divergent = false,
            .xff_raw = XFF_CLIENT,
            .fly_client_ip_raw = null,
        },
        .user_agent = "agentsfleet-cli/1.0",
    };
}

// ── failFromStoreError ───────────────────────────────────────────────────

const StoreArm = struct { err: anyerror, code: []const u8, detail: []const u8 };

const STORE_ARMS = [_]StoreArm{
    .{ .err = session_store.Error.InvalidPublicKey, .code = ec.ERR_INVALID_PUBLIC_KEY, .detail = "The supplied public_key is malformed" },
    .{ .err = session_store.Error.InvalidTokenName, .code = ec.ERR_INVALID_TOKEN_NAME, .detail = "token_name must be 1-64 characters of printable ASCII" },
    .{ .err = session_store.Error.InvalidCipherText, .code = ec.ERR_INVALID_CIPHERTEXT, .detail = "ciphertext is missing, empty, or malformed" },
    .{ .err = session_store.Error.InvalidNonce, .code = ec.ERR_INVALID_NONCE, .detail = "nonce is missing, empty, or the wrong length" },
    .{ .err = session_store.Error.InvalidVerificationCode, .code = ec.ERR_INVALID_VERIFICATION_CODE, .detail = "verification_code must be exactly 6 ASCII digits" },
    .{ .err = session_store.Error.AlreadyApproved, .code = ec.ERR_SESSION_ALREADY_APPROVED, .detail = "This login session has already been approved" },
    .{ .err = session_store.Error.SessionMissing, .code = ec.ERR_SESSION_NOT_FOUND, .detail = "Session was not found. It may have expired or been invalidated" },
    .{ .err = session_store.Error.NotOwner, .code = ec.ERR_FORBIDDEN, .detail = "You do not own this login session" },
    .{ .err = session_store.Error.SessionConsumed, .code = ec.ERR_SESSION_CONSUMED, .detail = "This login session has already been consumed" },
    .{ .err = session_store.Error.SessionAborted, .code = ec.ERR_SESSION_ABORTED, .detail = "This login session was aborted" },
};

/// The store failures with no caller-actionable classification. Each must land
/// on the generic internal answer.
const UNCLASSIFIED_STORE_ERRORS = [_]anyerror{
    session_store.Error.RedisError,
    session_store.Error.UnexpectedRedisReply,
    session_store.Error.OutOfMemory,
};

test "should answer each classified store failure with its own code and detail" {
    for (STORE_ARMS) |arm| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        var ctx = buildCtx();

        helpers.failFromStoreError(buildHx(ht.res, &ctx), arm.err, SESSION_ID);

        const json = try ht.getJson();
        testing.expectEqualStrings(arm.code, json.object.get(K_ERROR_CODE).?.string) catch |e| {
            std.debug.print("arm {s}: code mismatch\n", .{@errorName(arm.err)});
            return e;
        };
        try testing.expectEqualStrings(arm.detail, json.object.get(K_DETAIL).?.string);
    }
}

test "should not tell an unauthenticated caller which internal fault occurred" {
    for (UNCLASSIFIED_STORE_ERRORS) |err| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        var ctx = buildCtx();

        helpers.failFromStoreError(buildHx(ht.res, &ctx), err, SESSION_ID);

        const json = try ht.getJson();
        const detail = json.object.get(K_DETAIL).?.string;
        try testing.expectEqualStrings(ec.ERR_INTERNAL_OPERATION_FAILED, json.object.get(K_ERROR_CODE).?.string);
        // The Zig error tag is logged, never returned: a caller that can guess
        // "RedisError" from a 500 body learns the store's shape for free.
        try testing.expect(std.mem.indexOf(u8, detail, @errorName(err)) == null);
    }
}

test "should not echo the session id back when a store failure is reported" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx();

    helpers.failFromStoreError(buildHx(ht.res, &ctx), session_store.Error.NotOwner, SESSION_ID);

    const json = try ht.getJson();
    try testing.expect(std.mem.indexOf(u8, json.object.get(K_DETAIL).?.string, SESSION_ID) == null);
}

test "should report a store failure that carries no session id" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx();

    // The pre-lookup failures (a malformed public_key, a bad token_name) are
    // rejected before any session exists to name.
    helpers.failFromStoreError(buildHx(ht.res, &ctx), session_store.Error.InvalidPublicKey, null);

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_INVALID_PUBLIC_KEY, json.object.get(K_ERROR_CODE).?.string);
}

// ── dispatchVerifyOutcome ────────────────────────────────────────────────

fn dispatch(ht: *httpz.testing.Testing, outcome: session_store.VerifyOutcome) !std.json.Value {
    var ctx = buildCtx();
    helpers.dispatchVerifyOutcome(
        buildHx(ht.res, &ctx),
        outcome,
        SESSION_ID,
        "fp-" ++ "0" ** 8,
        buildScratchFixture(),
    );
    return ht.getJson();
}

const PAYLOAD = session_store.VerifyPayload{
    .dashboard_public_key = "pubkey-b64",
    .ciphertext = "ct-b64",
    .nonce = "nonce-b64",
};

test "should return the sealed payload when verification succeeds" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const json = try dispatch(&ht, .{ .success = PAYLOAD });

    try ht.expectStatus(200);
    try testing.expectEqualStrings(PAYLOAD.dashboard_public_key, json.object.get("dashboard_public_key").?.string);
    try testing.expectEqualStrings(PAYLOAD.ciphertext, json.object.get("ciphertext").?.string);
    try testing.expectEqualStrings(PAYLOAD.nonce, json.object.get("nonce").?.string);
}

test "should return the same payload on a replay inside the window" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // A CLI that retried after a dropped response must get the payload again
    // rather than a consumed error, or a lost reply costs the user their login.
    const json = try dispatch(&ht, .{ .replay = PAYLOAD });

    try ht.expectStatus(200);
    try testing.expectEqualStrings(PAYLOAD.ciphertext, json.object.get("ciphertext").?.string);
}

test "should answer a wrong code as retryable so the CLI can prompt again" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const json = try dispatch(&ht, .{ .invalid_code = 2 });

    try testing.expectEqualStrings(ec.ERR_VERIFICATION_FAILED, json.object.get(K_ERROR_CODE).?.string);
}

test "should answer the attempt that exhausts the limit as terminal, not retryable" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // The distinction that matters: ERR_VERIFICATION_FAILED here would have the
    // CLI keep prompting against a session the store already aborted.
    const json = try dispatch(&ht, .rate_limited);

    const code = json.object.get(K_ERROR_CODE).?.string;
    try testing.expectEqualStrings(ec.ERR_SESSION_ABORTED, code);
    try testing.expect(!std.mem.eql(u8, ec.ERR_VERIFICATION_FAILED, code));
}

test "should answer a verify against an unapproved session distinctly" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const json = try dispatch(&ht, .not_approved);

    try testing.expectEqualStrings(ec.ERR_SESSION_NOT_APPROVED, json.object.get(K_ERROR_CODE).?.string);
}

test "should map each terminal session state to its own code" {
    const Case = struct { outcome: session_store.VerifyOutcome, code: []const u8 };
    const cases = [_]Case{
        .{ .outcome = .{ .aborted = "operator revoked the session" }, .code = ec.ERR_SESSION_ABORTED },
        .{ .outcome = .consumed, .code = ec.ERR_SESSION_CONSUMED },
        .{ .outcome = .expired, .code = ec.ERR_SESSION_EXPIRED },
        .{ .outcome = .missing, .code = ec.ERR_SESSION_NOT_FOUND },
    };
    for (cases) |case| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();

        const json = try dispatch(&ht, case.outcome);

        try testing.expectEqualStrings(case.code, json.object.get(K_ERROR_CODE).?.string);
    }
}

test "should carry the abort reason through to the caller" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const reason = "operator revoked the session";

    const json = try dispatch(&ht, .{ .aborted = reason });

    try testing.expectEqualStrings(reason, json.object.get(K_DETAIL).?.string);
}

// ── scratch derivation + redaction ───────────────────────────────────────

test "should attribute the client to the forwarded-for header over the TCP peer" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(trusted_ip.HDR_XFF, XFF_CLIENT);
    ht.header(helpers.HDR_USER_AGENT, "agentsfleet-cli/9.9");

    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, ht.req);

    // Behind Fly the raw peer is the proxy, so an audit row that records it
    // attributes every login to the load balancer.
    try testing.expectEqualStrings(XFF_CLIENT, scratch.derived.ip);
    try testing.expectEqualStrings("agentsfleet-cli/9.9", scratch.user_agent);
}

test "should record an unknown user agent rather than an empty one" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var scratch: helpers.RequestScratch = undefined;
    helpers.buildScratch(&scratch, ht.req);

    // An absent header must not produce an empty attribution field — the audit
    // chain reads this and an empty string reads as "not captured".
    try testing.expectEqualStrings(helpers.S_USER_AGENT_UNKNOWN, scratch.user_agent);
}

test "should redact a session id instead of returning it whole" {
    var buf: [helpers.REDACT_BUF_LEN]u8 = undefined;

    const redacted = helpers.redactSid(&buf, SESSION_ID);

    try testing.expect(!std.mem.eql(u8, SESSION_ID, redacted));
    try testing.expect(redacted.len <= helpers.REDACT_BUF_LEN);
}
