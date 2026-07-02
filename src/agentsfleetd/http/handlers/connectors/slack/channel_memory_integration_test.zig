// Integration test — M106 §4 Dim 4.2: a Slack channel's durable memory is
// scoped to its resident fleet, so a fact captured in one thread is recalled in
// another. The memory endpoint is keyed ONLY by fleet_id (there is no thread
// column in memory.memory_entries), so "cross-thread" recall is structural: both
// a thread-A capture and a thread-B hydrate resolve the same channel_fleet_id.
//
// The test drives the real runner-plane memory loop (bearer auth, live PG):
//   • seed a resident channel fleet + its connector_channels binding,
//   • resolve the memory scope from the binding exactly as the server does,
//   • POST a fact (thread-A run capture) then GET it back (thread-B run hydrate).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const test_fixtures = @import("../../../../db/test_fixtures.zig");
const serve_runner_lookup = @import("../../../../cmd/serve_runner_lookup.zig");
const api_key = @import("../../../../auth/api_key.zig");
const spec = @import("spec.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;
const ALLOC = std.testing.allocator;

const TENANT_ID = "0195c106-6000-7000-8000-f00000000061"; // per-suite tenant — keeps this suite's workspace + lease off the shared tenant's FK chain
const TENANT_NAME = "slack-channel-memory-suite";
const WORKSPACE_ID = "0195c106-6001-7000-8000-000000000061";
const RUNNER_ID = "0195c106-6002-7000-8000-000000000062";
const FLEET_ID = "0195c106-6003-7000-8000-000000000063";
const LEASE_ID = "0195c106-6004-7000-8000-000000000064";
const BINDING_UID = "0195c106-6005-7000-8000-000000000065";
const FLEET_NAME = "slack-channel-t106mem-c106mem";
const TEAM_ID = "T106MEM";
const CHANNEL_ID = "C106MEM";
const EVENT_ID = "evt-chan-mem-1";
const FENCE: u64 = 7;
const NOW_MS: i64 = 1_900_000_000_000;
const MEM_KEY = "prod-name";
const MEM_CONTENT = "aurora";

const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "f" ** 64;

// SAFETY: populated by configureRegistry before the middleware chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedFleet(conn: *pg.Conn) !void {
    const now = common.clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, '# skill', '# trigger', '{}'::jsonb, 'active', $4, $4)
        \\ON CONFLICT (id) DO UPDATE SET status = 'active', updated_at = EXCLUDED.updated_at
    , .{ FLEET_ID, WORKSPACE_ID, FLEET_NAME, now });
}

fn seedBinding(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.connector_channels
        \\  (uid, provider, external_account_id, external_channel_id, fleet_id, kind, created_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5::uuid, 'resident', $6)
        \\ON CONFLICT (provider, external_account_id, external_channel_id) DO NOTHING
    , .{ BINDING_UID, spec.PROVIDER, TEAM_ID, CHANNEL_ID, FLEET_ID, common.clock.nowMillis() });
}

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'chan-mem-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

fn seedLease(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'slack:U1',
        \\        'chat', '{"message":"hi"}', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
        \\        $7, $8, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, TENANT_ID, EVENT_ID, @as(i64, FENCE), NOW_MS + 30_000 });
}

/// Best-effort cleanup exec — logs (never suppresses) so a stale-state warning
/// still surfaces without failing teardown (RULE: no `catch {}`).
fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn wipeMemory(conn: *pg.Conn) void {
    execIgnore(conn, "SET ROLE memory_runtime", .{});
    execIgnore(conn, "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "RESET ROLE", .{});
}

fn teardown(conn: *pg.Conn) void {
    wipeMemory(conn);
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM core.connector_channels WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID});
}

/// Resolve the memory scope from the channel binding — exactly the chain the
/// server walks (slack, team, channel) → channel_fleet_id.
fn scopeFromBinding(conn: *pg.Conn) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT fleet_id::text FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3",
        .{ spec.PROVIDER, TEAM_ID, CHANNEL_ID },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.NoBinding;
    return ALLOC.dupe(u8, try row.get([]const u8, 0));
}

fn memoryUrl(fleet_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/runners/me/memory/{s}", .{fleet_id});
}

fn capturePush() ![]u8 {
    return std.fmt.allocPrint(
        ALLOC,
        "{{\"lease_id\":\"{s}\",\"fencing_token\":{d},\"memory\":[" ++
            "{{\"key\":\"{s}\",\"content\":\"{s}\",\"category\":\"c\"}}]}}",
        .{ LEASE_ID, FENCE, MEM_KEY, MEM_CONTENT },
    );
}

fn startHarness() !?*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return null,
        else => return err,
    };
}

test "integration: a channel fact captured in one thread is recalled in another (Dim 4.2)" {
    const h = (try startHarness()) orelse return error.SkipZigTest;
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    teardown(conn);
    try seedFleet(conn);
    try seedBinding(conn);
    try seedRunner(conn);
    try seedLease(conn);
    defer teardown(conn);

    // The memory scope is the channel's resident fleet, resolved from the binding
    // — not any thread identifier (there is no thread dimension in the scope).
    const scope = try scopeFromBinding(conn);
    defer ALLOC.free(scope);
    try testing.expectEqualStrings(FLEET_ID, scope);
    const url = try memoryUrl(scope);
    defer ALLOC.free(url);

    // Thread-A run: capture a durable channel fact.
    const push = try capturePush();
    defer ALLOC.free(push);
    const cap = try (try (try h.post(url).bearer(RUNNER_TOKEN)).json(push)).send();
    defer cap.deinit();
    try cap.expectStatus(.ok);
    try testing.expect(cap.bodyContains("\"stored\":1"));

    // Thread-B run: a fresh hydrate on the SAME channel scope recalls the fact.
    // No thread id was ever supplied — recall crosses threads because the scope
    // is the channel fleet.
    const hyd = try (try h.get(url).bearer(RUNNER_TOKEN)).send();
    defer hyd.deinit();
    try hyd.expectStatus(.ok);
    try testing.expect(hyd.bodyContains(MEM_KEY));
    try testing.expect(hyd.bodyContains(MEM_CONTENT));
}
