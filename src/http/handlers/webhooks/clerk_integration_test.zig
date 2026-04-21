//! Integration tests for POST /v1/webhooks/clerk.
//!
//! Skips cleanly when TEST_DATABASE_URL is unset. Each test sets a deterministic
//! CLERK_WEBHOOK_SECRET before starting the harness, signs the payload with
//! hmac_sig.computeMac, and asserts both the response and the DB post-state.

const std = @import("std");
const pg = @import("pg");
const hs = @import("hmac_sig");
const c = @cImport(@cInclude("stdlib.h"));

const auth_mw = @import("../../../auth/middleware/mod.zig");
const svix = @import("../../../crypto/svix_verify.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

/// Raw key bytes (24 bytes) → base64 → `whsec_<base64>`. Mirrors the
/// svix_verify_test.zig pattern so both tests stay in sync.
const RAW_KEY: []const u8 = "0123456789abcdef01234567";
const WHSEC_KEY: []const u8 = "whsec_MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3";

const OIDC_HAPPY: []const u8 = "oidc-clerk-http-happy-01";
const OIDC_REPLAY: []const u8 = "oidc-clerk-http-replay-02";

fn noopConfigureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    _ = c.setenv("CLERK_WEBHOOK_SECRET", WHSEC_KEY.ptr, 1);
    return TestHarness.start(alloc, .{ .configureRegistry = noopConfigureRegistry });
}

fn unsetSecret() void {
    _ = c.unsetenv("CLERK_WEBHOOK_SECRET");
}

fn cleanupAccount(conn: *pg.Conn, oidc_subject: []const u8) void {
    _ = conn.exec(
        \\DELETE FROM core.workspaces
        \\WHERE tenant_id IN (SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch {};
    _ = conn.exec(
        \\DELETE FROM core.memberships
        \\WHERE user_id IN (SELECT user_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch {};
    _ = conn.exec(
        \\DELETE FROM core.tenants
        \\WHERE tenant_id IN (SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch {};
    _ = conn.exec("DELETE FROM core.users WHERE oidc_subject = $1", .{oidc_subject}) catch {};
}

/// Build a `v1,<base64_hmac>` entry against the test secret.
fn signEntry(alloc: std.mem.Allocator, id: []const u8, ts: []const u8, body: []const u8) ![]u8 {
    const mac = hs.computeMac(RAW_KEY, &.{ id, ".", ts, ".", body });
    const Encoder = std.base64.standard.Encoder;
    const enc_len = Encoder.calcSize(mac.len);
    const out = try alloc.alloc(u8, 3 + enc_len);
    @memcpy(out[0..3], "v1,");
    _ = Encoder.encode(out[3..], &mac);
    return out;
}

fn nowTsAlloc(alloc: std.mem.Allocator) ![]u8 {
    return std.fmt.allocPrint(alloc, "{d}", .{std.time.timestamp()});
}

fn userCreatedBody(alloc: std.mem.Allocator, clerk_user_id: []const u8, email: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc,
        \\{{"type":"user.created","data":{{"id":"{s}","email_addresses":[{{"id":"idn_x","email_address":"{s}"}}],"primary_email_address_id":"idn_x","first_name":"Happy","last_name":"Path"}}}}
    , .{ clerk_user_id, email });
}

fn countUsers(conn: *pg.Conn, oidc_subject: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.users WHERE oidc_subject = $1",
        .{oidc_subject},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return try row.get(i64, 0);
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "clerk webhook: valid signed user.created bootstraps and returns 200" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer unsetSecret();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, OIDC_HAPPY);
    }

    const svix_id = "msg_clerk_happy_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userCreatedBody(ALLOC, OIDC_HAPPY, "happy@acme.test");
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"created\":true"));
    try std.testing.expect(resp.bodyContains("\"workspace_name\""));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_HAPPY);
    try std.testing.expectEqual(@as(i64, 1), try countUsers(conn, OIDC_HAPPY));
}

test "clerk webhook: tampered body returns 401 UZ-WH-010 and writes no rows" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer unsetSecret();
    const oidc = "oidc-clerk-http-badsig-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_badsig_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const signed_body = try userCreatedBody(ALLOC, oidc, "badsig@acme.test");
    defer ALLOC.free(signed_body);
    const sig = try signEntry(ALLOC, svix_id, ts, signed_body);
    defer ALLOC.free(sig);
    // Send a DIFFERENT body than the one we signed. HMAC must reject.
    const tampered_body = try userCreatedBody(ALLOC, oidc, "tampered@acme.test");
    defer ALLOC.free(tampered_body);

    const resp = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(tampered_body)).send();
    defer resp.deinit();
    try resp.expectStatus(.unauthorized);
    try resp.expectErrorCode("UZ-WH-010");

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, oidc);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, oidc));
}

test "clerk webhook: stale timestamp returns 401 UZ-WH-011" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer unsetSecret();
    const oidc = "oidc-clerk-http-stale-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_stale_01";
    // 10 minutes in the past — well outside SVIX_MAX_DRIFT_SECONDS (300).
    const stale_ts = try std.fmt.allocPrint(ALLOC, "{d}", .{std.time.timestamp() - 600});
    defer ALLOC.free(stale_ts);
    const body = try userCreatedBody(ALLOC, oidc, "stale@acme.test");
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, svix_id, stale_ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, stale_ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.unauthorized);
    try resp.expectErrorCode("UZ-WH-011");

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, oidc);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, oidc));
}

test "clerk webhook: missing primary email returns 400 UZ-REQ-001" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer unsetSecret();
    const oidc = "oidc-clerk-http-noemail-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_noemail_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    // Valid JSON, valid sig, but no email addresses on the payload.
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"type":"user.created","data":{{"id":"{s}","email_addresses":[]}}}}
    , .{oidc});
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.bad_request);
    try resp.expectErrorCode("UZ-REQ-001");

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, oidc);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, oidc));
}

test "clerk webhook: replay of same user.created returns created:false with no new rows" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer unsetSecret();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, OIDC_REPLAY);
    }

    const body = try userCreatedBody(ALLOC, OIDC_REPLAY, "replay@acme.test");
    defer ALLOC.free(body);

    const first_ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(first_ts);
    const first_id = "msg_clerk_replay_a";
    const first_sig = try signEntry(ALLOC, first_id, first_ts, body);
    defer ALLOC.free(first_sig);
    const first = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, first_id))
        .header(svix.SVIX_TS_HEADER, first_ts))
        .header(svix.SVIX_SIG_HEADER, first_sig))
        .json(body)).send();
    defer first.deinit();
    try first.expectStatus(.ok);
    try std.testing.expect(first.bodyContains("\"created\":true"));

    // Second delivery — fresh svix_id/timestamp (Clerk retries pick a new id),
    // same event body. Handler's fast-path replay check should short-circuit.
    const second_ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(second_ts);
    const second_id = "msg_clerk_replay_b";
    const second_sig = try signEntry(ALLOC, second_id, second_ts, body);
    defer ALLOC.free(second_sig);
    const second = try (try (try (try (try h.post("/v1/webhooks/clerk")
        .header(svix.SVIX_ID_HEADER, second_id))
        .header(svix.SVIX_TS_HEADER, second_ts))
        .header(svix.SVIX_SIG_HEADER, second_sig))
        .json(body)).send();
    defer second.deinit();
    try second.expectStatus(.ok);
    try std.testing.expect(second.bodyContains("\"created\":false"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_REPLAY);
    try std.testing.expectEqual(@as(i64, 1), try countUsers(conn, OIDC_REPLAY));
}
