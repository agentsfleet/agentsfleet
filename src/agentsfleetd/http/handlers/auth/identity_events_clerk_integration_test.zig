//! Integration tests for POST /v1/auth/identity-events/clerk.
//!
//! Skips cleanly when TEST_DATABASE_URL is unset. Each test sets a deterministic
//! CLERK_WEBHOOK_SECRET before starting the harness, signs the payload with
//! hmac_sig.computeMac, and asserts both the response and the DB post-state.

const std = @import("std");
const common = @import("common");
const clock = @import("common").clock;
const pg = @import("pg");
const hs = @import("hmac_sig");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const env_resolve = @import("../../../config/env_resolve.zig");
const svix = @import("../../../auth/crypto/svix_verify.zig");
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
const OIDC_DELETE_AGENTS: []const u8 = "oidc-clerk-http-del-agt-03";

// Valid UUIDv7 (version char '7' at position 15) for the seeded fleet row;
// satisfies core.fleets' ck_fleets_uid_uuidv7 CHECK.
const SEED_FLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000903";

fn noopConfigureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopConfigureRegistry });
    h.ctx.clerk_webhook_secret = WHSEC_KEY;
    return h;
}

fn cleanupAccount(conn: *pg.Conn, oidc_subject: []const u8) void {
    // FK-safe order: fleets first (reference workspaces, no cascade), then
    // workspaces/memberships (reference tenant/user), then users + tenants in
    // a CTE so the RETURNING clause can feed the tenant delete after users are
    // gone. core.users.tenant_id → core.tenants has no ON DELETE CASCADE, so
    // tenants cannot drop while users reference them.
    _ = conn.exec(
        \\DELETE FROM core.fleets
        \\WHERE workspace_id IN (
        \\  SELECT workspace_id FROM core.workspaces
        \\  WHERE tenant_id IN (SELECT tenant_id FROM core.users WHERE oidc_subject = $1))
    , .{oidc_subject}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        \\DELETE FROM core.workspaces
        \\WHERE tenant_id IN (SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        \\DELETE FROM core.memberships
        \\WHERE user_id IN (SELECT user_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        \\WITH doomed_users AS (
        \\    DELETE FROM core.users WHERE oidc_subject = $1 RETURNING tenant_id
        \\)
        \\DELETE FROM core.tenants WHERE tenant_id IN (SELECT tenant_id FROM doomed_users)
    , .{oidc_subject}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
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
    return std.fmt.allocPrint(alloc, "{d}", .{clock.nowSeconds()});
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
    return row.get(i64, 0);
}

fn userDeletedBody(alloc: std.mem.Allocator, clerk_user_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc,
        \\{{"type":"user.deleted","data":{{"id":"{s}"}}}}
    , .{clerk_user_id});
}

/// First workspace of the subject's tenant. Caller owns the returned slice.
fn fetchWorkspaceId(conn: *pg.Conn, alloc: std.mem.Allocator, oidc_subject: []const u8) ![]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text FROM core.workspaces
        \\WHERE tenant_id = (SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
        \\LIMIT 1
    , .{oidc_subject}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceNotFound;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

fn insertFleet(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'purge-victim', '# z', '{}'::jsonb, 'active', 0, 0)
    , .{ fleet_id, workspace_id });
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "clerk webhook: valid signed user.created bootstraps and returns 200" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
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

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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

test "clerk webhook: a boot-resolved CLERK_WEBHOOK_SECRET authenticates a live webhook" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Source the secret through the real boot path — the canonical env-name const
    // + the fail-closed resolver — instead of the raw literal the other tests
    // pin. Proves the boot-resolved value drives a genuine Svix-signed request
    // through the full middleware → handler → DB chain.
    var env = try common.env.fromPairs(ALLOC, &.{.{ env_resolve.CLERK_WEBHOOK_SECRET_ENV, WHSEC_KEY }});
    defer env.deinit();
    const resolved = (try env_resolve.secret(&env, ALLOC, env_resolve.CLERK_WEBHOOK_SECRET_ENV)) orelse
        return error.TestUnexpectedResult;
    defer ALLOC.free(resolved);
    try std.testing.expectEqualStrings(WHSEC_KEY, resolved);
    h.ctx.clerk_webhook_secret = resolved;

    const oidc = "oidc-clerk-http-bootsecret-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_bootsecret_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userCreatedBody(ALLOC, oidc, "bootsecret@acme.test");
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"created\":true"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, oidc);
    try std.testing.expectEqual(@as(i64, 1), try countUsers(conn, oidc));
}

test "clerk webhook: tampered body returns 401 UZ-WH-010 and writes no rows" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
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

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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
    const oidc = "oidc-clerk-http-stale-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_stale_01";
    // 10 minutes in the past — well outside SVIX_MAX_DRIFT_SECONDS (300).
    const stale_ts = try std.fmt.allocPrint(ALLOC, "{d}", .{clock.nowSeconds() - 600});
    defer ALLOC.free(stale_ts);
    const body = try userCreatedBody(ALLOC, oidc, "stale@acme.test");
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, svix_id, stale_ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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

test "clerk webhook: oversized body returns 413 UZ-REQ-002 and writes no rows" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const oidc = "oidc-clerk-http-toobig-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    // Build a JSON body whose length equals MAX_BODY_SIZE (2 MiB). The handler's
    // checkBodySize fires on `body.len >= MAX_BODY_SIZE`. Padding lives inside a
    // string field so the document stays valid JSON and the parser ignores it.
    const max_body: usize = 2 * 1024 * 1024;
    const prefix = try std.fmt.allocPrint(ALLOC,
        \\{{"type":"user.created","data":{{"id":"{s}","email_addresses":[{{"id":"idn_x","email_address":"big@acme.test"}}],"primary_email_address_id":"idn_x"}},"_pad":"
    , .{oidc});
    defer ALLOC.free(prefix);
    const suffix: []const u8 = "\"}";
    std.debug.assert(prefix.len + suffix.len < max_body);
    const pad_len = max_body - prefix.len - suffix.len;

    const body = try ALLOC.alloc(u8, max_body);
    defer ALLOC.free(body);
    @memcpy(body[0..prefix.len], prefix);
    @memset(body[prefix.len .. prefix.len + pad_len], ' ');
    @memcpy(body[prefix.len + pad_len ..], suffix);

    const svix_id = "msg_clerk_toobig_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const sig = try signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.payload_too_large);
    try resp.expectErrorCode("UZ-REQ-002");

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, oidc);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, oidc));
}

test "clerk webhook: missing CLERK_WEBHOOK_SECRET fails closed with 500" {
    // Start the harness with ctx.clerk_webhook_secret left at its null default
    // so the handler's readSecret() fail-closed path runs. This guards the
    // security invariant that a misconfigured deploy returns 500 (not 401),
    // denying attackers a way to enumerate "is this endpoint configured?".
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopConfigureRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const oidc = "oidc-clerk-http-nosecret-01";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, oidc);
    }

    const svix_id = "msg_clerk_nosecret_01";
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userCreatedBody(ALLOC, oidc, "nosecret@acme.test");
    defer ALLOC.free(body);
    // Sig content is irrelevant — readSecret() fails before verification.
    const sig = try signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.internal_server_error);

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
    const first = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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
    const second = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
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

test "clerk webhook: user.deleted purges an account that still owns fleets" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_AGENTS);
    }

    // Bootstrap the account (tenant + workspace + user) via user.created.
    {
        const ts = try nowTsAlloc(ALLOC);
        defer ALLOC.free(ts);
        const body = try userCreatedBody(ALLOC, OIDC_DELETE_AGENTS, "delagt@acme.test");
        defer ALLOC.free(body);
        const sig = try signEntry(ALLOC, "msg_del_agt_acreate", ts, body);
        defer ALLOC.free(sig);
        const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
            .header(svix.SVIX_ID_HEADER, "msg_del_agt_acreate"))
            .header(svix.SVIX_TS_HEADER, ts))
            .header(svix.SVIX_SIG_HEADER, sig))
            .json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }

    // Seed a fleet in the account's workspace. core.fleets.workspace_id has
    // no ON DELETE CASCADE, so without child-first cleanup the workspace delete
    // throws an FK violation and the webhook 500s (Clerk then retries forever).
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const ws = try fetchWorkspaceId(conn, ALLOC, OIDC_DELETE_AGENTS);
        defer ALLOC.free(ws);
        try insertFleet(conn, ws, SEED_FLEET_ID);
    }

    // user.deleted must purge the whole account without an FK violation.
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userDeletedBody(ALLOC, OIDC_DELETE_AGENTS);
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, "msg_del_agt_adelete", ts, body);
    defer ALLOC.free(sig);
    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, "msg_del_agt_adelete"))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"deleted\":true"));

    // The cascade reached the user/tenant: a rollback (the old bug) would have
    // left the user row intact.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_AGENTS);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_AGENTS));
}
