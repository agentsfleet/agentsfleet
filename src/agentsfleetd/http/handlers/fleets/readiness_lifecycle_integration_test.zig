// Readiness-index cleanup across the fleet lifecycle (M141 §3, Dimension 3.7).
//
// The clear inside the lease poll is reachable only for fleets the candidate
// query returns, and that query filters `status = 'active'`. So the instant a
// fleet is deleted or leaves `active`, its readiness field becomes unreachable
// and nothing can ever remove it — it holds a slot of the bounded `HRANDFIELD`
// peek sample forever. These proofs pin the two lifecycle sites that close it.
//
// Tenant + workspace are the ones the shared scope-token personas carry in
// their `metadata` claim — a workspace-scoped route 403s against any other
// pair, so these ids are fixed by the auth fixture, not chosen. Deliberately
// NOT the shared `TEST_TENANT_ID` from `db/test_fixtures.zig`, and teardown
// leaves the tenant and workspace in place: sibling suites key off the same
// persona rows, and deleting them is the leak that makes
// `secret_probe.resolvePrimaryWorkspace` resolve someone else's workspace.
//
// Requires TEST_DATABASE_URL + a reachable Redis — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const queue_consts = @import("../../../queue/constants.zig");
const redis_fleet = @import("../../../queue/redis_fleet.zig");

const ALLOC = std.testing.allocator;

// Fixed by the scope-token `metadata` claim (see module note).
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// Distinct node suffix (…0a7c…) so no sibling fleet suite collides on these rows.
const FLEET_DELETED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7c01";
const FLEET_STOPPED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7c02";

const TOKEN = scope_fixtures.OPERATOR; // fleet:write
const TOKEN_ADMIN = scope_fixtures.TENANT_ADMIN; // fleet:admin — delete needs it

const CMD_HGET = "HGET";
const CMD_EXISTS = "EXISTS";

const CONFIG_JSON =
    \\{"name":"readiness-lifecycle-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: readiness-lifecycle-bot
    \\---
    \\
    \\You are a readiness lifecycle test fleet.
;

/// `config_json` is a STRING field on the PATCH body, so the config travels
/// JSON-escaped inside it. A `\\` literal performs no escape processing, so the
/// `\"` sequences below reach the wire verbatim — which is what the handler
/// parses back out.
const PATCH_CONFIG_ONLY_BODY =
    \\{"config_json":"{\"name\":\"readiness-lifecycle-bot\",\"x-agentsfleet\":{\"triggers\":[{\"type\":\"webhook\",\"source\":\"agentmail\"}],\"tools\":[\"agentmail\"],\"budget\":{\"daily_dollars\":5.0}}}"}
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedTenantAndWorkspace(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'ReadinessLifecycle', 0, 0) ON CONFLICT (tenant_id) DO NOTHING
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2::uuid, 0) ON CONFLICT (workspace_id) DO NOTHING
    , .{ WORKSPACE_ID, TENANT_ID });
}

fn seedFleet(conn: *pg.Conn, fleet_id: []const u8, name: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, status, config_json, source_markdown,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'active', $4, $5, $6, $6)
        \\ON CONFLICT (id) DO UPDATE SET status = 'active'
    , .{ fleet_id, WORKSPACE_ID, name, CONFIG_JSON, SOURCE_MD, clock.nowMillis() });
}

/// True when `fleet_id` currently holds a field in the readiness index.
fn isMarked(h: *TestHarness, fleet_id: []const u8) !bool {
    var resp = try h.queue.command(&.{ CMD_HGET, queue_consts.ready_index_key, fleet_id });
    defer resp.deinit(h.queue.alloc);
    return switch (resp) {
        .bulk => |v| v != null,
        else => false,
    };
}

/// True when the fleet's event stream key still exists.
fn streamExists(h: *TestHarness, fleet_id: []const u8) !bool {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try h.queue.command(&.{ CMD_EXISTS, key });
    defer resp.deinit(h.queue.alloc);
    return switch (resp) {
        .integer => |v| v > 0,
        else => false,
    };
}

/// Append a real event through the one producer, which is what marks the fleet
/// ready — asserting against a hand-written HSET would not prove the production
/// path leaves anything behind.
fn publishEvent(h: *TestHarness, fleet_id: []const u8) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, fleet_id);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
        .event_id = "",
        .fleet_id = fleet_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    for ([_][]const u8{ FLEET_DELETED, FLEET_STOPPED }) |fid| {
        // The production purge, not a hand-rolled DEL: deleting a stream also
        // deletes its consumer group, and only `purgeFleetRedisState` drops the
        // process-global group memo alongside it. A raw DEL leaves the memo
        // claiming a group that no longer exists, which costs the next suite to
        // reuse this fleet id a whole poll spent discovering `NOGROUP`.
        redis_fleet.purgeFleetRedisState(&h.queue, fid) catch |err| {
            std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
        };
        execIgnore(conn, "DELETE FROM core.fleet_sessions WHERE fleet_id = $1::uuid", .{fid});
        execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{fid});
    }
    // Tenant + workspace deliberately survive — see the module note. They are
    // the scope personas' rows, shared with every other workspace-scoped suite.
}

test "integration: deleting a fleet clears its readiness mark and its stream" {
    const h = makeHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedTenantAndWorkspace(conn);
    try seedFleet(conn, FLEET_DELETED, "readiness-lifecycle-bot");
    try publishEvent(h, FLEET_DELETED);
    try std.testing.expect(try isMarked(h, FLEET_DELETED));

    // DELETE requires the fleet be `killed` first (delete.zig gates on it).
    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}", .{ WORKSPACE_ID, FLEET_DELETED });
    defer ALLOC.free(url);
    const killed = try (try (try h.patch(url).bearer(TOKEN)).json("{\"status\":\"killed\"}")).send();
    defer killed.deinit();
    try killed.expectStatus(.ok);

    const removed = try (try h.delete(url).bearer(TOKEN_ADMIN)).send();
    defer removed.deinit();
    try std.testing.expectEqual(@as(u16, 204), removed.status);

    // The row is gone, so the candidate query can never return this fleet and
    // the poll-site clear is unreachable for it. Both Redis traces must go.
    try std.testing.expect(!try isMarked(h, FLEET_DELETED));
    try std.testing.expect(!try streamExists(h, FLEET_DELETED));
}

test "integration: stopping a fleet clears its readiness mark but keeps the stream" {
    const h = makeHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedTenantAndWorkspace(conn);
    try seedFleet(conn, FLEET_STOPPED, "readiness-lifecycle-bot");
    try publishEvent(h, FLEET_STOPPED);
    try std.testing.expect(try isMarked(h, FLEET_STOPPED));

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}", .{ WORKSPACE_ID, FLEET_STOPPED });
    defer ALLOC.free(url);
    const stopped = try (try (try h.patch(url).bearer(TOKEN)).json("{\"status\":\"stopped\"}")).send();
    defer stopped.deinit();
    try stopped.expectStatus(.ok);

    // Unleasable ⇒ unreachable by the poll-site clear ⇒ the mark must go here.
    try std.testing.expect(!try isMarked(h, FLEET_STOPPED));
    // But the event is NOT lost: a resume must still find it, and the sweeper's
    // undelivered probe re-marks the fleet once it is `active` again.
    try std.testing.expect(try streamExists(h, FLEET_STOPPED));
}

test "integration: a config-only PATCH leaves an active fleet's readiness mark intact" {
    const h = makeHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedTenantAndWorkspace(conn);
    try seedFleet(conn, FLEET_STOPPED, "readiness-lifecycle-bot");
    try publishEvent(h, FLEET_STOPPED);
    try std.testing.expect(try isMarked(h, FLEET_STOPPED));

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}", .{ WORKSPACE_ID, FLEET_STOPPED });
    defer ALLOC.free(url);
    // No `status` field — the fleet stays active and genuinely still holds work,
    // so clearing here would strand the event until a sweep. Guards the
    // regression where the clear fires on every PATCH.
    const patched = try (try (try h.patch(url).bearer(TOKEN)).json(PATCH_CONFIG_ONLY_BODY)).send();
    defer patched.deinit();
    try patched.expectStatus(.ok);

    try std.testing.expect(try isMarked(h, FLEET_STOPPED));
}
