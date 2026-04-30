// Integration tests for M41 §7 continuation flow against real Postgres + Redis.
//
// The pure-decision side (`continuation.zig`) has unit tests pinning the
// classifier verdict for every (stage_result, prior_count) combination. The
// wire side (`event_loop_continuation.zig`) needs real DB + Redis to verify
// the recursive-CTE chain count, the XADD landing shape (covered by unit
// tests on `event_envelope.encodeForXAdd`), and the failure_label UPDATE on
// chain force-stop.
//
// Mapping to the spec's Test Specification table (Acceptance Criteria #1):
//
//   landed here:
//     test_continuation_event_resumes      → §7 enqueue happy path
//     test_max_continuation_chain_10       → §7 Invariant 4 force-stop at 11th
//     test_config_hot_reload_next_event    → §9 reloadZombieConfig swaps session.config
//
//   covered by unit tests instead (see capabilities.md §4 chain-cap section
//   for the [CHANGED] semantics — runtime can't mutate NullClaw conversation
//   so §4/§5/§6 became observability counters + SKILL prose):
//     test_secret_substitution_real_bytes_outbound  → policy_http_request_test.zig
//                                                     (real-network E2E unreachable in
//                                                     `zig build test`: NullClaw's
//                                                     HttpRequestTool short-circuits at
//                                                     `builtin.is_test` before any wire
//                                                     dispatch; closing it requires a
//                                                     vendored fork or out-of-test binary)
//     test_secret_no_leak_into_event_log            → unit-tested via redactBytes
//     test_secret_no_leak_into_agent_context        → policy_http_request_test.zig
//     test_network_allow_blocks_off_list            → policy_http_request_test.zig
//     test_tool_window_drops_old_results            → runner_progress_test.zig
//     test_memory_checkpoint_nudge_fires            → runner_progress_test.zig
//     test_stage_chunk_at_threshold                 → runner_progress_test.zig
//
// Requires LIVE_DB=1 (TEST_DATABASE_URL or DATABASE_URL) + a reachable Redis
// (TEST_REDIS_TLS_URL). Skipped when either is missing.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const Allocator = std.mem.Allocator;

const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const executor_client = @import("../executor/client.zig");

const types = @import("event_loop_types.zig");
const event_loop = @import("event_loop.zig");
const event_loop_continuation = @import("event_loop_continuation.zig");
const base = @import("../db/test_fixtures.zig");
const helpers = @import("test_harness_helpers.zig");

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f30";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa30";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa31";
const ORIGIN_EVENT_ID = "1000000000000-0";
const STEER_ACTOR = "steer:integration-test";
const CHAIN_ESCALATE_LABEL = "chunk_chain_escalate_human";
const STREAM_KEY = "zombie:" ++ TEST_ZOMBIE_ID ++ ":events";

const ZOMBIE_NAME = "context-lifecycle-bot";
// Post-M46: runtime keys live under x-usezombie:. `chat` trigger type isn't
// in the parser's accepted set yet (api/webhook/cron/chain only), so this
// fixture uses `api` — the test exercises config swap mechanics, not trigger
// dispatch.
const ZOMBIE_CONFIG_JSON =
    \\{"name":"context-lifecycle-bot","x-usezombie":{"trigger":{"type":"api"},"tools":["http_request"],"budget":{"daily_dollars":1.0}}}
;
const ZOMBIE_SOURCE_MD =
    \\---
    \\name: context-lifecycle-bot
    \\---
    \\
    \\You are a context lifecycle integration test agent.
;

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", STREAM_KEY }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

/// Insert a row into core.zombie_events at a chosen chain position. Pass null
/// for `parent_event_id` when seeding the chain origin (a regular steer); pass
/// the immediate parent's event_id for a continuation row so the recursive
/// CTE in countPriorContinuationsForChain walks the full lineage.
fn insertEventRow(
    conn: *pg.Conn,
    event_id: []const u8,
    actor: []const u8,
    event_type: []const u8,
    parent_event_id: ?[]const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (zombie_id, event_id, workspace_id, actor, event_type,
        \\   status, request_json, resumes_event_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, 'processed', '{}'::jsonb, $6, $7, $7)
        \\ON CONFLICT (zombie_id, event_id) DO NOTHING
    , .{ TEST_ZOMBIE_ID, event_id, TEST_WORKSPACE_ID, actor, event_type, parent_event_id, now_ms });
}

fn streamLen(redis: *queue_redis.Client) !i64 {
    var resp = try redis.command(&.{ "XLEN", STREAM_KEY });
    defer resp.deinit(redis.alloc);
    return switch (resp) {
        .integer => |n| n,
        else => 0,
    };
}

fn readFailureLabel(conn: *pg.Conn, event_id: []const u8) !?[]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT failure_label FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ TEST_ZOMBIE_ID, event_id }));
    defer q.deinit();
    if (try q.next()) |row| {
        const v = try row.get(?[]const u8, 0);
        if (v) |s| return try ALLOC.dupe(u8, s);
    }
    return null;
}

fn fakeChunkStage() executor_client.ExecutorClient.StageResult {
    return .{
        .content = "needs continuation",
        .token_count = 200,
        .wall_seconds = 2,
        .exit_ok = false,
        .failure = null,
        .checkpoint_id = "ckpt-incident:findings",
    };
}

fn buildEvent(event_id: []const u8, actor: []const u8, event_type: []const u8) redis_zombie.ZombieEvent {
    return .{
        .event_id = @constCast(event_id),
        .actor = @constCast(actor),
        .event_type = @constCast(event_type),
        .workspace_id = @constCast(TEST_WORKSPACE_ID),
        .request_json = @constCast("{}"),
        .created_at_ms = 0,
    };
}

fn buildSession() types.ZombieSession {
    return .{
        .zombie_id = TEST_ZOMBIE_ID,
        .workspace_id = TEST_WORKSPACE_ID,
        .config = undefined,
        .instructions = "",
        .context_json = "",
        .source_markdown = "",
    };
}

// ── Test 1: §7 enqueue happy path ──────────────────────────────────────────

test "integration: continuation re-enqueue lands an event on zombie:{id}:events" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = blk: {
        const tls_url = std.process.getEnvVarOwned(ALLOC, helpers.REDIS_TLS_URL_ENV_VAR) catch return error.SkipZigTest;
        defer ALLOC.free(tls_url);
        break :blk queue_redis.Client.connectFromUrl(ALLOC, tls_url) catch return error.SkipZigTest;
    };
    defer redis.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, ZOMBIE_NAME, ZOMBIE_CONFIG_JSON, ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    // Originating event row — the steer that the agent voluntarily chunks.
    try insertEventRow(db_ctx.conn, ORIGIN_EVENT_ID, STEER_ACTOR, "chat", null);
    defer cleanupZombieEventsRows(db_ctx.conn);

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);

    var session = buildSession();
    const event = buildEvent(ORIGIN_EVENT_ID, STEER_ACTOR, "chat");

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .redis_publish = &redis,
        .executor = undefined, // §7 path never reaches the executor — only PG count + Redis XADD + PG UPDATE
        .running = &running,
    };

    const stage = fakeChunkStage();
    event_loop_continuation.run(ALLOC, cfg, &session, &event, &stage);

    // Stream should now carry exactly one continuation envelope. The
    // envelope's wire shape is unit-tested in event_envelope_test.zig
    // (encodeForXAdd produces type/actor/workspace_id/request_json/
    // created_at fields); this test pins the count + the trigger
    // condition (exit_ok=false + non-empty checkpoint_id at chain origin).
    try std.testing.expectEqual(@as(i64, 1), try streamLen(&redis));

    // Originating row should NOT carry the chain-cap label (count was 0,
    // verdict was enqueue, not force_stop).
    const label = try readFailureLabel(db_ctx.conn, ORIGIN_EVENT_ID);
    defer if (label) |l| ALLOC.free(l);
    try std.testing.expect(label == null);
}

// ── Test 2: §7 Invariant 4 — force-stop at 11th continuation ──────────────

test "integration: 11th continuation force-stops with chunk_chain_escalate_human label, no XADD" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = blk: {
        const tls_url = std.process.getEnvVarOwned(ALLOC, helpers.REDIS_TLS_URL_ENV_VAR) catch return error.SkipZigTest;
        defer ALLOC.free(tls_url);
        break :blk queue_redis.Client.connectFromUrl(ALLOC, tls_url) catch return error.SkipZigTest;
    };
    defer redis.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, ZOMBIE_NAME, ZOMBIE_CONFIG_JSON, ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    // Build a chain of 11 rows: origin + 10 continuations linked back via
    // resumes_event_id. The 10th continuation's stage just finished and
    // would produce the 11th — that's the one §7 must refuse to enqueue.
    cleanupZombieEventsRows(db_ctx.conn);
    defer cleanupZombieEventsRows(db_ctx.conn);

    try insertEventRow(db_ctx.conn, ORIGIN_EVENT_ID, STEER_ACTOR, "chat", null);
    var id_storage: [11][32]u8 = undefined;
    var ids: [11][]const u8 = undefined;
    ids[0] = ORIGIN_EVENT_ID;
    var prev: []const u8 = ids[0];
    var i: usize = 1;
    while (i <= 10) : (i += 1) {
        const id = try std.fmt.bufPrint(&id_storage[i], "100000000000{d}-0", .{i});
        ids[i] = id;
        try insertEventRow(db_ctx.conn, id, "continuation:" ++ STEER_ACTOR, "continuation", prev);
        prev = id;
    }

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);

    var session = buildSession();
    const event = buildEvent(ids[10], "continuation:" ++ STEER_ACTOR, "continuation");

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .redis_publish = &redis,
        .executor = undefined,
        .running = &running,
    };

    const stage = fakeChunkStage();
    event_loop_continuation.run(ALLOC, cfg, &session, &event, &stage);

    // Stream MUST be empty — Invariant 4 forbids the 11th XADD.
    try std.testing.expectEqual(@as(i64, 0), try streamLen(&redis));

    // The originating row (the 10th continuation, whose stage just finished)
    // must carry the chain-cap label so the operator sees it in
    // `zombiectl events {id}` and the activity stream's terminal frame.
    const label = try readFailureLabel(db_ctx.conn, ids[10]);
    defer if (label) |l| ALLOC.free(l);
    try std.testing.expect(label != null);
    try std.testing.expectEqualStrings(CHAIN_ESCALATE_LABEL, label.?);
}

// ── Test 3: §9 config hot-reload ───────────────────────────────────────────

const NAME_SEEDED = "ctx-bot-seeded";
const NAME_RELOADED = "ctx-bot-reloaded";
const CONFIG_SEEDED =
    \\{"name":"ctx-bot-seeded","x-usezombie":{"trigger":{"type":"api"},"tools":["http_request"],"budget":{"daily_dollars":1.0}}}
;
const CONFIG_RELOADED =
    \\{"name":"ctx-bot-reloaded","x-usezombie":{"trigger":{"type":"api"},"tools":["http_request"],"budget":{"daily_dollars":2.0}}}
;

test "integration: reloadZombieConfig swaps session.config after DB row UPDATE" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, NAME_SEEDED, CONFIG_SEEDED, ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);
    try std.testing.expectEqualStrings(NAME_SEEDED, session.config.name);

    _ = try db_ctx.conn.exec(
        "UPDATE core.zombies SET config_json = $2::jsonb WHERE id = $1::uuid",
        .{ TEST_ZOMBIE_ID, CONFIG_RELOADED },
    );

    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);
    try std.testing.expectEqualStrings(NAME_RELOADED, session.config.name);
}
