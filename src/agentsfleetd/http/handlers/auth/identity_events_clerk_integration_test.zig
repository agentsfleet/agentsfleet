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
const Credentials = @import("../../../cron/Credentials.zig");
const QStashClient = @import("../../../cron/QStashClient.zig");
const Store = @import("../../../cron/Store.zig");
const metrics = @import("../../../observability/metrics_counters.zig");
const fixtures = @import("../../../db/test_fixtures.zig");

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

// ── §7 — teardown unregisters upstream schedule timers before it purges ──
//
// The bug these cover: a purge cascaded the tenant's schedule ROWS away but
// never told the provider, so an erased tenant kept a cron firing at a runner
// forever. The provider client is faked at its `Exchange` seam — the same seam
// `schedules/api_integration_test.zig` uses — because the assertion is about
// WHEN we call the provider relative to the row purge, not about its wire
// format.

const OIDC_DELETE_SCHED: []const u8 = "oidc-clerk-http-del-sched-04";
const OIDC_DELETE_REPLAY: []const u8 = "oidc-clerk-http-del-replay-05";
const OIDC_DELETE_FAIL: []const u8 = "oidc-clerk-http-del-fail-06";
const OIDC_DELETE_STRAND: []const u8 = "oidc-clerk-http-del-strand-07";
const OIDC_DELETE_NOCREDS: []const u8 = "oidc-clerk-http-del-nocreds-08";
const OIDC_DELETE_ONESLOT: []const u8 = "oidc-clerk-http-del-oneslot-09";
const SCHED_FLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000904";
const SCHED_SCHEDULE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000905";
const SCHED_LEASE_TOKEN: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000906";
const SCHED_SCHEDULE_ID_TWO: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000907";
const SCHED_LEASE_TOKEN_TWO: []const u8 = "0195b4ba-8d3a-7f13-8abc-000000000908";
/// Two schedules on the fleet, so both must reach the provider.
const STRANDED_EXPECTED_DELETES: u32 = 2;
/// Ceiling on the pool-drain loop — the harness pool is far smaller.
const MAX_HELD_CONNS: usize = 32;
/// A short acquire budget so a regression fails fast instead of stalling on
/// the two-second production default.
const FAST_ACQUIRE_TIMEOUT_NS: u64 = 200 * std.time.ns_per_ms;
const QSTASH_API_BASE: []const u8 = "https://qstash.teardown.test";
const SCHED_CRON: []const u8 = "0 9 * * *";
const SCHED_TIMEZONE: []const u8 = "Asia/Kolkata";
const SCHED_MESSAGE: []const u8 = "summarize";
const SCHED_CREATED_AT_MS: i64 = 100;
const SCHED_SYNCED_AT_MS: i64 = 101;
const SCHED_LEASE_UNTIL_MS: i64 = 200;
const PROVIDER_FAILURE_STATUS: u16 = 500;

/// The provider, faked at the boundary. `rows_at_first_call` is the assertion
/// that matters: it samples the tenant's surviving schedule rows AT THE MOMENT
/// the provider is called, so "unregister happens before the purge" is proven
/// by observation rather than by reading the handler top to bottom.
///
/// The probe deliberately rides its OWN connection, not the harness pool. This
/// callback fires on the request thread mid-unregister, and reaching into the
/// pool from here would put the assertion in competition with the code it is
/// measuring — a starved pool would surface as an unrelated `ConnectionBusy`
/// somewhere later in the suite rather than as a failure of this test. Its
/// independence is also what lets the single-slot test below drain the harness
/// pool without blinding this probe.
const FakeQStash = struct {
    probe: *pg.Conn,
    status: std.atomic.Value(u16) = .init(204),
    deletes: std.atomic.Value(u32) = .init(0),
    rows_at_first_call: std.atomic.Value(i64) = .init(-1),

    fn exchange(self: *FakeQStash) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *FakeQStash = @ptrCast(@alignCast(ptr));
        if (request.method == .DELETE) {
            if (self.deletes.fetchAdd(1, .monotonic) == 0) {
                self.rows_at_first_call.store(self.liveScheduleRows(), .release);
            }
        }
        const status = self.status.load(.acquire);
        if (status != 200 and status != 204) return .{ .status = status, .body = try alloc.dupe(u8, "{}") };
        return .{ .status = 204, .body = try alloc.dupe(u8, "") };
    }

    /// Schedule rows still on disk, or -1 if the probe itself could not run.
    fn liveScheduleRows(self: *FakeQStash) i64 {
        var q = PgQuery.from(self.probe.query(
            "SELECT COUNT(*)::bigint FROM core.fleet_schedules WHERE fleet_id = $1::uuid",
            .{SCHED_FLEET_ID},
        ) catch return -1);
        defer q.deinit();
        const row = (q.next() catch return -1) orelse return -1;
        return row.get(i64, 0) catch -1;
    }
};

fn testCredentials() !Credentials {
    const token = try ALLOC.dupe(u8, "teardown-qstash-token");
    errdefer ALLOC.free(token);
    const current = try ALLOC.dupe(u8, "teardown-current-signing-key");
    errdefer ALLOC.free(current);
    const next = try ALLOC.dupe(u8, "teardown-next-signing-key");
    errdefer ALLOC.free(next);
    return .{
        .token = token,
        .current_signing_key = current,
        .next_signing_key = next,
        .url = try ALLOC.dupe(u8, QSTASH_API_BASE),
    };
}

/// A fleet in the account's workspace carrying one SYNCED schedule — the state
/// that leaves a live upstream timer behind if teardown skips the unregister.
fn seedFleetWithSchedule(h: *TestHarness, conn: *pg.Conn, oidc_subject: []const u8) !void {
    const ws = try fetchWorkspaceId(conn, ALLOC, oidc_subject);
    defer ALLOC.free(ws);
    try insertFleet(conn, ws, SCHED_FLEET_ID);
    try addSyncedSchedule(h, SCHED_SCHEDULE_ID, SCHED_LEASE_TOKEN);
}

/// One more SYNCED schedule on the same fleet. Two is the minimum that can show
/// whether a failed unregister strands its siblings.
fn addSyncedSchedule(h: *TestHarness, schedule_id: []const u8, lease_token: []const u8) !void {
    const store = Store.init(h.ctx.pool);
    var created = switch (try store.create(ALLOC, .{
        .fleet_id = SCHED_FLEET_ID,
        .source = .api,
        .source_key = schedule_id,
        .cron = SCHED_CRON,
        .timezone = SCHED_TIMEZONE,
        .message = SCHED_MESSAGE,
    }, schedule_id, lease_token, SCHED_CREATED_AT_MS, SCHED_LEASE_UNTIL_MS)) {
        .created => |schedule| schedule,
        else => return error.ScheduleCreateFailed,
    };
    defer created.deinit(ALLOC);
    var synced = (try store.finalizeSuccess(ALLOC, schedule_id, created.generation, lease_token, SCHED_SYNCED_AT_MS)) orelse
        return error.ScheduleFinalizeFailed;
    synced.deinit(ALLOC);
}

fn countSchedules(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM core.fleet_schedules WHERE fleet_id = $1::uuid",
        .{SCHED_FLEET_ID},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.ScheduleCountMissing;
    return row.get(i64, 0);
}

/// Bootstrap an account through the real `user.created` webhook.
fn bootstrapAccount(h: *TestHarness, oidc_subject: []const u8, msg_id: []const u8, email: []const u8) !void {
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userCreatedBody(ALLOC, oidc_subject, email);
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, msg_id, ts, body);
    defer ALLOC.free(sig);
    const resp = try (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, msg_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
}

/// Deliver one signed `user.deleted`; caller owns the returned response.
fn deliverUserDeleted(h: *TestHarness, oidc_subject: []const u8, msg_id: []const u8) !harness_mod.Response {
    const ts = try nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const body = try userDeletedBody(ALLOC, oidc_subject);
    defer ALLOC.free(body);
    const sig = try signEntry(ALLOC, msg_id, ts, body);
    defer ALLOC.free(sig);
    return (try (try (try (try h.post("/v1/auth/identity-events/clerk")
        .header(svix.SVIX_ID_HEADER, msg_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
}

const TeardownSetup = struct {
    h: *TestHarness,
    creds: *Credentials,
    fake: *FakeQStash,
    probe_pool: *pg.Pool,
    probe_conn: *pg.Conn,

    fn init() !TeardownSetup {
        const h = startHarness(ALLOC) catch |err| switch (err) {
            error.SkipZigTest => return error.SkipZigTest,
            else => return err,
        };
        errdefer h.deinit();
        const probe = (try fixtures.openTestConn(ALLOC)) orelse return error.SkipZigTest;
        errdefer {
            probe.pool.release(probe.conn);
            probe.pool.deinit();
        }
        const creds = try ALLOC.create(Credentials);
        errdefer ALLOC.destroy(creds);
        creds.* = try testCredentials();
        errdefer creds.deinit(ALLOC);
        const fake = try ALLOC.create(FakeQStash);
        errdefer ALLOC.destroy(fake);
        fake.* = .{ .probe = probe.conn };
        h.ctx.qstash_credentials = creds;
        h.ctx.qstash_exchange_override = fake.exchange();
        metrics.resetRunnerMaintenanceMetricsForTest();
        return .{
            .h = h,
            .creds = creds,
            .fake = fake,
            .probe_pool = probe.pool,
            .probe_conn = probe.conn,
        };
    }

    fn deinit(self: *TeardownSetup) void {
        self.h.deinit();
        self.probe_pool.release(self.probe_conn);
        self.probe_pool.deinit();
        self.creds.deinit(ALLOC);
        ALLOC.destroy(self.creds);
        ALLOC.destroy(self.fake);
        self.* = undefined;
    }
};

test "integration: teardown unregisters the tenant's schedules BEFORE it purges the rows" {
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_SCHED);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_SCHED, "msg_del_sched_create", "delsched@acme.test");
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        try seedFleetWithSchedule(setup.h, conn, OIDC_DELETE_SCHED);
        try std.testing.expectEqual(@as(i64, 1), try countSchedules(conn));
    }

    const resp = try deliverUserDeleted(setup.h, OIDC_DELETE_SCHED, "msg_del_sched_delete");
    defer resp.deinit();
    try resp.expectStatus(.ok);

    // The provider heard about it...
    try std.testing.expect(setup.fake.deletes.load(.acquire) >= 1);
    // ...while the row it names still existed. A zero here is the original bug:
    // the rows cascade away and the upstream timer is never told.
    try std.testing.expectEqual(@as(i64, 1), setup.fake.rows_at_first_call.load(.acquire));

    const conn = try setup.h.acquireConn();
    defer setup.h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_SCHED);
    try std.testing.expectEqual(@as(i64, 0), try countSchedules(conn));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_SCHED));
}

test "integration: replaying user.deleted is a no-op the second time" {
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_REPLAY);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_REPLAY, "msg_del_replay_create", "delreplay@acme.test");

    const first = try deliverUserDeleted(setup.h, OIDC_DELETE_REPLAY, "msg_del_replay_1");
    defer first.deinit();
    try first.expectStatus(.ok);
    const deletes_after_first = setup.fake.deletes.load(.acquire);

    // Clerk retries on any non-2xx, so the second delivery must answer 200 with
    // nothing left to do — not 404, not 500, and not a second provider call for
    // schedules that no longer exist.
    const second = try deliverUserDeleted(setup.h, OIDC_DELETE_REPLAY, "msg_del_replay_2");
    defer second.deinit();
    try second.expectStatus(.ok);
    try std.testing.expectEqual(deletes_after_first, setup.fake.deletes.load(.acquire));

    const conn = try setup.h.acquireConn();
    defer setup.h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_REPLAY);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_REPLAY));
}

test "integration: a provider unregister failure is counted, and the purge still happens" {
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_FAIL);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_FAIL, "msg_del_fail_create", "delfail@acme.test");
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        try seedFleetWithSchedule(setup.h, conn, OIDC_DELETE_FAIL);
    }
    setup.fake.status.store(PROVIDER_FAILURE_STATUS, .release);

    const resp = try deliverUserDeleted(setup.h, OIDC_DELETE_FAIL, "msg_del_fail_delete");
    defer resp.deinit();
    // Erasure wins: a third party being down never blocks a deletion request.
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"deleted\":true"));

    // The counter is the reconciliation signal — a replay cannot retry this,
    // because by then the schedule rows are gone and there is nothing to
    // enumerate. If this stays zero, a leaked upstream timer is invisible.
    try std.testing.expect(metrics.snapshot().account_teardown_unregister_failures_total >= 1);

    const conn = try setup.h.acquireConn();
    defer setup.h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_FAIL);
    try std.testing.expectEqual(@as(i64, 0), try countSchedules(conn));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_FAIL));
}

test "integration: a failed unregister does not strand the schedules behind it" {
    // `removeAll` used to return on the FIRST provider failure, so schedule two
    // was never attempted — and the purge then deleted the row naming it. One
    // transient 500 leaked every timer behind it, permanently and silently.
    // Two schedules, provider failing for all of them: both must still be
    // called.
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_STRAND);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_STRAND, "msg_del_strand_create", "delstrand@acme.test");
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        try seedFleetWithSchedule(setup.h, conn, OIDC_DELETE_STRAND);
        try addSyncedSchedule(setup.h, SCHED_SCHEDULE_ID_TWO, SCHED_LEASE_TOKEN_TWO);
        try std.testing.expectEqual(@as(i64, STRANDED_EXPECTED_DELETES), try countSchedules(conn));
    }
    setup.fake.status.store(PROVIDER_FAILURE_STATUS, .release);

    const resp = try deliverUserDeleted(setup.h, OIDC_DELETE_STRAND, "msg_del_strand_delete");
    defer resp.deinit();
    try resp.expectStatus(.ok);

    // The assertion the old shape fails: every schedule reached the provider,
    // not just the one before the first failure.
    try std.testing.expectEqual(STRANDED_EXPECTED_DELETES, setup.fake.deletes.load(.acquire));
    try std.testing.expect(metrics.snapshot().account_teardown_unregister_failures_total >= 1);

    const conn = try setup.h.acquireConn();
    defer setup.h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_STRAND);
    try std.testing.expectEqual(@as(i64, 0), try countSchedules(conn));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_STRAND));
}

test "integration: teardown unregisters with only one pool connection free" {
    // The pool-exhaustion class, made deterministic. `runDelete` used to hold a
    // connection across the whole unregister pass while `cron_sync` reached for
    // a second, so a request needed TWO slots at once. Production defaults to
    // four slots and a two-second acquire timeout while admitting far more
    // concurrent handlers, which meant a handful of simultaneous deletions
    // could each hold one slot, time every nested acquire out, and purge
    // anyway — timers alive, retry state erased, nothing logged as unusual.
    //
    // One free slot is the smallest fixture that tells the two shapes apart:
    // the staged version completes, the nested one cannot.
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_ONESLOT);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_ONESLOT, "msg_del_oneslot_create", "deloneslot@acme.test");
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        try seedFleetWithSchedule(setup.h, conn, OIDC_DELETE_ONESLOT);
    }

    // Take everything, then hand exactly one back.
    setup.h.pool._timeout = FAST_ACQUIRE_TIMEOUT_NS;
    var held: [MAX_HELD_CONNS]?*pg.Conn = @splat(null);
    var held_count: usize = 0;
    while (held_count < MAX_HELD_CONNS) {
        held[held_count] = setup.h.pool.acquire() catch break;
        held_count += 1;
    }
    try std.testing.expect(held_count >= 1);
    held_count -= 1;
    setup.h.pool.release(held[held_count].?);
    held[held_count] = null;
    defer for (held[0..held_count]) |maybe| {
        if (maybe) |c| setup.h.pool.release(c);
    };

    const resp = try deliverUserDeleted(setup.h, OIDC_DELETE_ONESLOT, "msg_del_oneslot_delete");
    defer resp.deinit();
    try resp.expectStatus(.ok);

    // The provider heard about it on one slot. A zero here means the nested
    // acquire timed out and the timer outlived the account.
    try std.testing.expect(setup.fake.deletes.load(.acquire) >= 1);
    try std.testing.expectEqual(@as(u64, 0), metrics.snapshot().account_teardown_unregister_failures_total);
}

test "integration: missing provider credentials count as a leak, not as silence" {
    // QStash credentials go absent after a startup vault or database fault.
    // Teardown mapped that to `.unconfigured` and said nothing, which is only
    // true when nothing was ever registered — but the schedule ROWS are right
    // there, and the timers they name were registered during an earlier healthy
    // run. A transient restart fault therefore turned every subsequent account
    // deletion into an invisible upstream leak. The purge still proceeds
    // (erasure wins); what changes is that the leak is now counted.
    var setup = TeardownSetup.init() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer setup.deinit();
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        cleanupAccount(conn, OIDC_DELETE_NOCREDS);
    }
    try bootstrapAccount(setup.h, OIDC_DELETE_NOCREDS, "msg_del_nocreds_create", "delnocreds@acme.test");
    {
        const conn = try setup.h.acquireConn();
        defer setup.h.releaseConn(conn);
        try seedFleetWithSchedule(setup.h, conn, OIDC_DELETE_NOCREDS);
    }
    // Exactly the state a startup credential-load failure leaves behind.
    setup.h.ctx.qstash_credentials = null;

    const resp = try deliverUserDeleted(setup.h, OIDC_DELETE_NOCREDS, "msg_del_nocreds_delete");
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"deleted\":true"));

    // Nothing could be sent, and that is precisely what must be counted.
    try std.testing.expectEqual(@as(u32, 0), setup.fake.deletes.load(.acquire));
    try std.testing.expect(metrics.snapshot().account_teardown_unregister_failures_total >= 1);

    const conn = try setup.h.acquireConn();
    defer setup.h.releaseConn(conn);
    defer cleanupAccount(conn, OIDC_DELETE_NOCREDS);
    try std.testing.expectEqual(@as(i64, 0), try countSchedules(conn));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC_DELETE_NOCREDS));
}
