// Wire-level redaction tests. Spawns `zombied-executor-stub` (production
// NullClaw pipeline + observer + redactor adapter all live, only the LLM
// provider is swapped for the canned-response stub), drives a StartStage
// with `agent_config.api_key = SYNTHETIC_SECRET`, and asserts on the
// bytes the worker reads off the executor RPC socket:
//
//   - the placeholder `${secrets.llm.api_key}` appears in the captured
//     stream (redactor substituted as expected);
//   - the resolved secret bytes never appear (no leak through tool_use
//     args, agent_response_chunk content, terminal StageResponse, or any
//     other frame the executor emitted during the run).
//
// The stub provider's canned response carries the synthetic secret in
// BOTH `content` (textDelta path) AND tool_call `arguments` (tool_use
// observer path), so a single run exercises both redactor branches.

const std = @import("std");
const Allocator = std.mem.Allocator;

const canary = @import("../executor/redaction_canary.zig");
const Harness = @import("test_executor_harness.zig");
const RpcRecorder = @import("test_rpc_recorder.zig");

const ALLOC = std.testing.allocator;

const SYNTHETIC_SECRET = canary.SYNTHETIC_SECRET;
const PLACEHOLDER = canary.PLACEHOLDER;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f40";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
const TEST_TRACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa42";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa41";

const RedactionRun = struct {
    harness: Harness,
    recorder: RpcRecorder,

    fn deinit(self: *RedactionRun) void {
        self.recorder.uninstall();
        self.recorder.deinit();
        self.harness.deinit();
    }
};

fn driveRedactionRun(alloc: Allocator) !RedactionRun {
    var harness = try Harness.start(alloc, .{ .binary = .stub });
    errdefer harness.deinit();

    var recorder = RpcRecorder.init(alloc);
    errdefer recorder.deinit();
    recorder.install();
    errdefer recorder.uninstall();

    const execution_id = try harness.executor.createExecution(.{
        .workspace_path = "/tmp",
        .correlation = .{
            .trace_id = TEST_TRACE_ID,
            .zombie_id = TEST_ZOMBIE_ID,
            .workspace_id = TEST_WORKSPACE_ID,
            .session_id = TEST_SESSION_ID,
        },
    });
    defer alloc.free(execution_id);
    defer harness.executor.destroyExecution(execution_id) catch {};

    // The stub provider's canned response will fire tool_use + chunk
    // observer events containing SYNTHETIC_SECRET. Tool dispatch may
    // fail (no matching tool in the empty tools list) — we don't care:
    // the redactor runs BEFORE dispatch, so the bytes on the wire are
    // already the assertion target.
    if (harness.executor.startStage(execution_id, .{
        .agent_config = .{
            .model = "stub",
            .provider = "stub",
            .api_key = SYNTHETIC_SECRET,
        },
        .message = "redact me",
    })) |result| {
        alloc.free(result.content);
        if (result.checkpoint_id) |c| alloc.free(c);
    } else |_| {}

    return .{ .harness = harness, .recorder = recorder };
}

test "test_executor_args_redacted_at_sandbox_boundary" {
    var run = try driveRedactionRun(ALLOC);
    defer run.deinit();

    try std.testing.expect(run.recorder.contains(PLACEHOLDER));
    try std.testing.expect(!run.recorder.contains(SYNTHETIC_SECRET));
}

test "test_executor_passes_through_redacted_for_chunks" {
    // Same canned run as the boundary test — the stub's `content` field
    // (the textDelta-path payload) carries SYNTHETIC_SECRET literally,
    // so the chunk-redaction branch fires alongside the tool-arg branch
    // in a single run. This block isolates the chunk-side claim: no
    // agent_response_chunk frame leaks the secret.
    var run = try driveRedactionRun(ALLOC);
    defer run.deinit();

    try std.testing.expect(!run.recorder.contains(SYNTHETIC_SECRET));
}

// ── Pub/sub-side assertion (test row 2) ─────────────────────────────────────
//
// Drives the full worker writepath against the stub-provider executor and
// asserts no Redis publish frame on `zombie:{id}:activity` carries the
// resolved secret. Skip-safe locally: needs Postgres + a TLS Redis URL
// (`TEST_REDIS_TLS_URL`) — both absent on a fresh checkout, so the test
// returns `error.SkipZigTest` without spinning up infrastructure.

const event_loop = @import("event_loop.zig");
const writepath = @import("event_loop_writepath.zig");
const types = @import("event_loop_types.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const EventEnvelope = @import("event_envelope.zig");
const base = @import("../db/test_fixtures.zig");
const helpers = @import("test_harness_helpers.zig");
const crypto_store = @import("../secrets/crypto_store.zig");

const TEST_ACTOR = "steer:test-user";
const TEST_REQUEST_JSON = "{\"message\":\"redact me\"}";
const EMPTY_CONTEXT_JSON = "{}";
const TEST_CONSUMER = "redaction-pubsub-consumer";
const ACTIVITY_DRAIN_BUDGET_MS: u64 = 5_000;

// Zombie config that declares a credential named "llm_key". The writepath's
// resolveFirstCredential iterates session.config.credentials and looks up
// each name in vault.secrets keyed `zombie:{name}` — we seed exactly that
// row below with SYNTHETIC_SECRET as plaintext.
const REDACTION_ZOMBIE_NAME = "redaction-bot";
const REDACTION_ZOMBIE_CONFIG_JSON =
    "{\"name\":\"" ++ REDACTION_ZOMBIE_NAME ++
    "\",\"x-usezombie\":{\"trigger\":{\"type\":\"webhook\",\"source\":\"agentmail\"}," ++
    "\"tools\":[\"agentmail\"],\"budget\":{\"daily_dollars\":5.0}," ++
    "\"credentials\":[\"llm_key\"]}}";
const REDACTION_ZOMBIE_SOURCE_MD = "---\nname: " ++ REDACTION_ZOMBIE_NAME ++ "\n---\n\nYou are a redaction bot.\n";

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *@import("pg").Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

test "test_args_redacted_no_secret_leak" {
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

    var subscriber = (try helpers.connectSubscriber(ALLOC)) orelse return error.SkipZigTest;
    defer subscriber.deinit();
    try subscriber.subscribe("zombie:" ++ TEST_ZOMBIE_ID ++ ":activity");
    std.Thread.sleep(helpers.SUBSCRIBE_SETTLE_MS * std.time.ns_per_ms);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    // Platform provider for billing + model_caps; the api_key it stores is
    // unused on the redaction path (we resolve via resolveFirstCredential
    // below), but the writepath's debit/balance gates need the row.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownPlatformProvider(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, REDACTION_ZOMBIE_NAME, REDACTION_ZOMBIE_CONFIG_JSON, REDACTION_ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, EMPTY_CONTEXT_JSON);
    // Plant SYNTHETIC_SECRET at vault.secrets[(workspace, "zombie:llm_key")]
    // — the exact slot resolveFirstCredential reads, given the zombie
    // config above declares "llm_key" in its credentials list. Without
    // this, the executor receives an empty api_key, collectSecrets has
    // nothing to scrub, and the test would falsely catch its own setup.
    base.setTestEncryptionKey();
    try crypto_store.store(ALLOC, db_ctx.conn, TEST_WORKSPACE_ID, "zombie:llm_key", SYNTHETIC_SECRET);
    defer {
        _ = db_ctx.conn.exec(
            "DELETE FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2",
            .{ TEST_WORKSPACE_ID, "zombie:llm_key" },
        ) catch {};
    }

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);
    defer cleanupZombieEventsRows(db_ctx.conn);

    try redis_zombie.ensureZombieConsumerGroup(&redis, TEST_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .binary = .stub });
    defer harness.deinit();

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = TEST_ZOMBIE_ID,
        .workspace_id = TEST_WORKSPACE_ID,
        .actor = TEST_ACTOR,
        .event_type = .chat,
        .request_json = TEST_REQUEST_JSON,
        .created_at = std.time.milliTimestamp(),
    };
    const xadded_id = try redis.xaddZombieEvent(envelope);
    defer ALLOC.free(xadded_id);

    var evt = (try redis_zombie.xreadgroupZombie(&redis, TEST_ZOMBIE_ID, TEST_CONSUMER)) orelse
        return error.UnexpectedNullEvent;
    defer evt.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .redis_publish = &redis,
        .executor = harness.executorPtr(),
        .running = &running,
        .balance_policy = .stop,
    };
    var consec: u32 = 0;
    _ = writepath.run(ALLOC, &session, &evt, cfg, &consec);

    // Drain raw payloads off the activity channel. Three assertions:
    //   1. (negative) SYNTHETIC_SECRET never appears in any frame
    //   2. (presence) at least one frame carries the placeholder, proving
    //      the redactor actually substituted (not just dropped the payload)
    //   3. (positive control) frame_count > 0, proving the writepath
    //      didn't silently short-circuit and trivially-pass the negative
    const overall_deadline = std.time.milliTimestamp() + @as(i64, @intCast(ACTIVITY_DRAIN_BUDGET_MS));
    var last_msg_at = std.time.milliTimestamp();
    var frame_count: u32 = 0;
    var saw_placeholder = false;
    while (std.time.milliTimestamp() < overall_deadline) {
        const msg_opt = try subscriber.readMessage();
        if (msg_opt) |m| {
            var msg = m;
            defer msg.deinit();
            try std.testing.expect(std.mem.indexOf(u8, msg.data, SYNTHETIC_SECRET) == null);
            if (std.mem.indexOf(u8, msg.data, PLACEHOLDER) != null) saw_placeholder = true;
            frame_count += 1;
            last_msg_at = std.time.milliTimestamp();
            continue;
        }
        if (std.time.milliTimestamp() - last_msg_at > @as(i64, @intCast(helpers.DRAIN_QUIET_MS))) break;
    }
    try std.testing.expect(frame_count > 0);
    try std.testing.expect(saw_placeholder);
}

