// Integration tests — M106 §2 signed events ingress. Drives the real
// POST /v1/connectors/slack/events route through TestHarness (live Postgres +
// Redis), signing each request with the platform Slack signing secret seeded in
// the admin-workspace `slack-app` vault entry.
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.
//
// Covers: Dim 2.1 (signed app_mention → 200 + one stream entry, materializing
// the resident fleet) folded with Dim 3.1 (a second mention reuses the same
// fleet); Dim 2.2 (unmapped team → 200-ack no-op + bad signature → 401
// end-to-end); Dim 2.3 (url_verification handshake echoes the challenge).

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const test_fixtures = @import("../../../../db/test_fixtures.zig");
const hs = @import("hmac_sig");
const id_format = @import("../../../../types/id_format.zig");
const ec = @import("../../../../errors/error_registry.zig");
const slack_sig = @import("slack_sig.zig");
const spec = @import("spec.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

// UUIDv7-shaped fixtures distinct from the oauth-callback suite's ids so the
// shared test DB stays collision-free under the parallel runner.
const TENANT_ID = "0195c106-1000-7000-8000-f00000000011"; // per-suite tenant — keeps this suite's workspaces off the shared tenant's FK chain
const TENANT_NAME = "slack-events-suite";
const ADMIN_WS = "0195c106-1001-7000-8000-000000000011";
const TARGET_WS = "0195c106-1002-7000-8000-000000000012";
const SIGNING_SECRET = "m106-events-signing-secret-key!!";
const TEAM_ID = "T106EVT";
const TEAM_UNMAPPED = "T106NONE";
const CHANNEL_ID = "C106EVT";
const RESIDENT_NAME = "slack-channel-t106evt-c106evt"; // "slack-channel-" ++ lowercase(TEAM_ID) ++ "-" ++ lowercase(CHANNEL_ID)
const USER_ID = "U777";
const EVENTS_PATH = "/v1/connectors/slack/events";

// Dim 3.2 concurrent-first-mention fixtures — a distinct (team, channel) from
// the Dim 2.1 suite above so the two tests never contend for the same resident
// fleet name under the parallel runner. Both mentions below target THIS channel.
const TEAM_CC = "T106CC";
const CHANNEL_CC = "C106CC";
const RESIDENT_NAME_CC = "slack-channel-t106cc-c106cc"; // "slack-channel-" ++ lower(TEAM_CC) ++ "-" ++ lower(CHANNEL_CC)

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

// ── Fixtures ─────────────────────────────────────────────────────────────────

fn seedSlackApp(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "client_id", .{ .string = "test-client-id" });
    try obj.put(alloc, "client_secret", .{ .string = "test-client-secret" });
    try obj.put(alloc, "signing_secret", .{ .string = SIGNING_SECRET });
    try test_fixtures.storeVaultJson(alloc, conn, ADMIN_WS, "slack-app", .{ .object = obj });
}

const INSERT_INSTALL_SQL =
    \\INSERT INTO core.connector_installs
    \\  (uid, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7)
    \\ON CONFLICT (provider, external_account_id) DO UPDATE SET workspace_id = EXCLUDED.workspace_id
;

fn seedInstall(alloc: std.mem.Allocator, conn: *pg.Conn, team_id: []const u8, ws: []const u8) !void {
    const uid = try id_format.generateConnectorInstallId(alloc);
    defer alloc.free(uid);
    const scopes: []const []const u8 = &.{ "app_mentions:read", "chat:write" };
    _ = try conn.exec(INSERT_INSTALL_SQL, .{ uid, spec.PROVIDER, team_id, ws, "UADMIN", scopes, common.clock.nowMillis() });
}

fn preClean(conn: *pg.Conn) void {
    // Deleting the resident fleet cascades its connector_channels binding (FK
    // ON DELETE CASCADE); the explicit binding delete covers a run that never
    // materialized. Installs for both teams are cleared so each test seeds its
    // own precondition.
    _ = conn.exec("DELETE FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_ID }) catch |e| std.log.warn("preclean channels: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2", .{ TARGET_WS, RESIDENT_NAME }) catch |e| std.log.warn("preclean fleet: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_ID }) catch |e| std.log.warn("preclean install1: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_UNMAPPED }) catch |e| std.log.warn("preclean install2: {s}", .{@errorName(e)});
}

/// Post-test teardown for a materialized resident fleet. It is born ACTIVE with a
/// fresh event on its Redis stream, so leaving it behind lets an unrelated
/// suite's runner lease-scan pick it up (control_plane's "assigns across active
/// fleets" asserts an exact fleet count under seed-randomized test order). Delete
/// the fleet_events + the fleet so it drops out of the active-fleet scan; an
/// orphaned Redis stream with no backing PG fleet is never scanned, so it is inert.
fn teardownResident(conn: *pg.Conn, name: []const u8, team: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id IN (SELECT id FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2)", .{ TARGET_WS, name }) catch |e| std.log.warn("teardown events: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, team }) catch |e| std.log.warn("teardown channels: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2", .{ TARGET_WS, name }) catch |e| std.log.warn("teardown fleet: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, team }) catch |e| std.log.warn("teardown install: {s}", .{@errorName(e)});
}

// ── Request signing + assertions ─────────────────────────────────────────────

fn mentionBody(alloc: std.mem.Allocator, team: []const u8, channel: []const u8, ts: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"type\":\"event_callback\",\"team_id\":\"{s}\"," ++
            "\"event\":{{\"type\":\"app_mention\",\"channel\":\"{s}\",\"user\":\"{s}\",\"text\":\"<@U0BOT> hi\",\"ts\":\"{s}\"}}}}",
        .{ team, channel, USER_ID, ts },
    );
}

/// Sign `body` as Slack does (`v0=` ++ hex(HMAC(secret, v0:ts:body))) and POST it.
/// `sig`/`ts` slices live on this frame and outlive the inner `send()`.
fn postSigned(h: *TestHarness, secret: []const u8, now_s: i64, body: []const u8) !harness_mod.Response {
    var ts_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{now_s});
    // Sign the way the ingress verifies: reuse the connector's single sig config.
    const mac = hs.computeMac(secret, &.{ slack_sig.CONFIG.hmac_version, ":", ts, ":", body });
    var sig_buf: [slack_sig.CONFIG.prefix.len + hs.MAC_LEN * 2]u8 = undefined;
    const sig = hs.encodeMacHex(&sig_buf, slack_sig.CONFIG.prefix, mac);
    var rq = h.post(EVENTS_PATH);
    rq = try rq.header(ec.SLACK_SIG_HEADER, sig);
    rq = try rq.header(ec.SLACK_TS_HEADER, ts);
    rq = rq.rawBody(body);
    return rq.send();
}

/// Assert exactly one (slack, TEAM_ID, CHANNEL_ID) binding and return its
/// owned fleet_id (caller frees).
fn oneBinding(alloc: std.mem.Allocator, conn: *pg.Conn) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT fleet_id::text FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3",
        .{ spec.PROVIDER, TEAM_ID, CHANNEL_ID },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.NoBinding;
    const fid = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(fid);
    if ((try q.next()) != null) return error.MultipleBindings;
    return fid;
}

fn streamLen(h: *TestHarness, alloc: std.mem.Allocator, fleet_id: []const u8) !i64 {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var resp = try h.queue.command(&.{ "XLEN", key });
    defer resp.deinit(alloc);
    return switch (resp) {
        .integer => |n| n,
        else => error.UnexpectedXlenResp,
    };
}

fn countRows(conn: *pg.Conn, sql: []const u8, args: anytype) !i64 {
    var q = PgQuery.from(try conn.query(sql, args));
    defer q.deinit();
    const row = try q.next() orelse return error.CountRowMissing;
    return row.get(i64, 0);
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

test "integration: signed app_mention acks + enqueues; second mention reuses the fleet (Dim 2.1/3.1)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    preClean(conn);
    defer teardownResident(conn, RESIDENT_NAME, TEAM_ID); // don't leak the materialized fleet into other lease scans
    try seedSlackApp(alloc, conn);
    try seedInstall(alloc, conn, TEAM_ID, TARGET_WS);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    const now = common.clock.nowSeconds();

    // Mention #1 → materializes the resident fleet + binding, enqueues one event.
    const body1 = try mentionBody(alloc, TEAM_ID, CHANNEL_ID, "1700000000.000200");
    defer alloc.free(body1);
    const r1 = try postSigned(h, SIGNING_SECRET, now, body1);
    defer r1.deinit();
    try r1.expectStatus(.ok);

    const fleet_id = try oneBinding(alloc, conn);
    defer alloc.free(fleet_id);
    // The resident fleet row exists with the channel-derived name (via the shared
    // insertFleetOnConn path — never a direct core.fleets insert).
    const name = try countRows(conn, "SELECT count(*) FROM core.fleets WHERE id = $1::uuid AND name = $2", .{ fleet_id, RESIDENT_NAME });
    try testing.expectEqual(@as(i64, 1), name);
    // Activation (§4): the resident fleet is `active` (leaseable) right after
    // materialization — NOT left `installing`. A reactive fleet has no
    // provisioning beat, so channel_fleet activates it inline before binding.
    const active = try countRows(conn, "SELECT count(*) FROM core.fleets WHERE id = $1::uuid AND status = 'active'", .{fleet_id});
    try testing.expectEqual(@as(i64, 1), active);
    // One stream entry landed before the response.
    try testing.expectEqual(@as(i64, 1), try streamLen(h, alloc, fleet_id));

    // Mention #2 (distinct event.ts) → same fleet, still one binding, two entries.
    const body2 = try mentionBody(alloc, TEAM_ID, CHANNEL_ID, "1700000000.000300");
    defer alloc.free(body2);
    const r2 = try postSigned(h, SIGNING_SECRET, now, body2);
    defer r2.deinit();
    try r2.expectStatus(.ok);

    const fleet_id2 = try oneBinding(alloc, conn);
    defer alloc.free(fleet_id2);
    try testing.expectEqualStrings(fleet_id, fleet_id2); // reused, not re-materialized
    try testing.expectEqual(@as(i64, 2), try streamLen(h, alloc, fleet_id));
}

test "integration: unmapped team is a 200-ack no-op, nothing enqueued (Dim 2.2)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    preClean(conn);
    try seedSlackApp(alloc, conn); // signing secret present, but NO install for TEAM_UNMAPPED
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const body = try mentionBody(alloc, TEAM_UNMAPPED, "C000NONE", "1700000000.000900");
    defer alloc.free(body);
    const r = try postSigned(h, SIGNING_SECRET, common.clock.nowSeconds(), body);
    defer r.deinit();
    try r.expectStatus(.ok); // 200-ack — Slack must never see an error loop
    try testing.expect(r.bodyContains(ec.ERR_SLACK_TEAM_NOT_INSTALLED));

    const bindings = try countRows(conn, "SELECT count(*) FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_UNMAPPED });
    try testing.expectEqual(@as(i64, 0), bindings);
}

test "integration: url_verification handshake echoes the challenge (Dim 2.3)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try seedSlackApp(alloc, conn);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const body = "{\"type\":\"url_verification\",\"challenge\":\"abc123XYZchallenge\"}";
    const r = try postSigned(h, SIGNING_SECRET, common.clock.nowSeconds(), body);
    defer r.deinit();
    try r.expectStatus(.ok);
    try testing.expect(r.bodyContains("abc123XYZchallenge"));
}

test "integration: a bad signature is rejected 401 UZ-SLK-010 end-to-end (Dim 2.2)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try seedSlackApp(alloc, conn);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    const body = try mentionBody(alloc, TEAM_ID, CHANNEL_ID, "1700000000.000200");
    defer alloc.free(body);
    // Signed with the WRONG secret → the endpoint's verify fails against the
    // vaulted signing secret.
    const r = try postSigned(h, "totally-wrong-secret", common.clock.nowSeconds(), body);
    defer r.deinit();
    try r.expectStatus(.unauthorized);
    try r.expectErrorCode(ec.ERR_SLACK_SIG_INVALID);
}

// ── Dim 3.2 — concurrent first-mention converges on exactly one fleet ─────────

/// One barrier-gated firing of a signed mention. Both worker threads spin on the
/// shared gate and cross it together (Zig 0.16 dropped `ResetEvent.timedWait`, so
/// an atomic-bool gate is the house barrier idiom — see
/// patch_concurrent_integration_test.zig), maximizing the odds that both reach
/// `channel_fleet.materialize` before either inserts its binding — i.e. that the
/// race actually exercises the fleet-name-unique (23505) convergence path rather
/// than the trivial "second mention just reads the binding" path. The invariant
/// under test (one fleet + one binding) holds either way; the gate only sharpens
/// which code path is covered. `status` is left 0 on any error so the test fails.
const ConcurrentFirstMention = struct {
    fn fire(h: *TestHarness, now_s: i64, body: []const u8, gate: *std.atomic.Value(bool), status: *u16) void {
        while (!gate.load(.acquire)) std.atomic.spinLoopHint();
        const r = postSigned(h, SIGNING_SECRET, now_s, body) catch return;
        defer r.deinit();
        status.* = r.status;
    }
};

test "integration: two concurrent first-mentions converge on exactly one fleet + binding (Dim 3.2)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    // Pre-clean this channel's rows (fleet delete cascades its binding via the FK;
    // the explicit binding + install deletes cover a never-materialized prior run).
    _ = conn.exec("DELETE FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_CC }) catch |e| std.log.warn("cc preclean channels: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2", .{ TARGET_WS, RESIDENT_NAME_CC }) catch |e| std.log.warn("cc preclean fleet: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_CC }) catch |e| std.log.warn("cc preclean install: {s}", .{@errorName(e)});
    defer teardownResident(conn, RESIDENT_NAME_CC, TEAM_CC); // this active fleet + its Redis event is control_plane's phantom 3rd lease
    try seedSlackApp(alloc, conn);
    try seedInstall(alloc, conn, TEAM_CC, TARGET_WS);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    const now = common.clock.nowSeconds();

    // Two distinct events (distinct event.ts → distinct dedup keys, both enqueue)
    // racing to materialize the SAME channel's resident fleet.
    const body1 = try mentionBody(alloc, TEAM_CC, CHANNEL_CC, "1700000001.000100");
    defer alloc.free(body1);
    const body2 = try mentionBody(alloc, TEAM_CC, CHANNEL_CC, "1700000001.000200");
    defer alloc.free(body2);

    var status: [2]u16 = .{ 0, 0 };
    var gate = std.atomic.Value(bool).init(false);
    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, ConcurrentFirstMention.fire, .{ h, now, body1, &gate, &status[0] });
    threads[1] = try std.Thread.spawn(.{}, ConcurrentFirstMention.fire, .{ h, now, body2, &gate, &status[1] });
    gate.store(true, .release); // release both threads together
    for (threads) |t| t.join();

    // Both mentions are acked 200 — the race loser converges, it does not error.
    try testing.expectEqual(@as(u16, 200), status[0]);
    try testing.expectEqual(@as(u16, 200), status[1]);

    // Invariant 6 (one resident fleet per channel under concurrency) + Invariant 1
    // (one binding): the fleet-name unique constraint serializes the two inserts;
    // the loser resolves the winner's fleet and the binding is ON CONFLICT DO NOTHING.
    const fleet_count = try countRows(conn, "SELECT count(*) FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2", .{ TARGET_WS, RESIDENT_NAME_CC });
    try testing.expectEqual(@as(i64, 1), fleet_count);
    const binding_count = try countRows(conn, "SELECT count(*) FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3", .{ spec.PROVIDER, TEAM_CC, CHANNEL_CC });
    try testing.expectEqual(@as(i64, 1), binding_count);
}
