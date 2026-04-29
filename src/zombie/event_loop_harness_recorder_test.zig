// Wire-level capture test: verifies the RpcRecorder fixture taps
// `protocol.readFrameFromFd` correctly by scripting the harness binary
// to emit a tool_call_started frame whose `args_redacted` field carries
// a `${secrets.fly.api_token}` placeholder, then asserting the worker's
// captured byte stream contains exactly the placeholder.
//
// This is a fixture sanity check, NOT a redaction test. The placeholder
// is supplied verbatim by the script — there is no NullClaw runner
// resolving secrets. The full production-binary redaction tests
// (test_executor_args_redacted_at_sandbox_boundary,
// test_args_redacted_no_secret_leak) are deferred to a follow-up spec
// because the harness strips the redaction code path entirely.

const std = @import("std");
const Allocator = std.mem.Allocator;

const event_loop = @import("event_loop.zig");
const writepath = @import("event_loop_writepath.zig");
const types = @import("event_loop_types.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const EventEnvelope = @import("event_envelope.zig");
const base = @import("../db/test_fixtures.zig");
const Harness = @import("test_executor_harness.zig");
const helpers = @import("test_harness_helpers.zig");
const RpcRecorder = @import("test_rpc_recorder.zig");

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f40";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa41";
const TEST_ACTOR = "steer:test-user";
const TEST_REQUEST_JSON = "{\"message\":\"redaction-fixture\"}";
const EMPTY_CONTEXT_JSON = "{}";
const TEST_CONSUMER = "harness-recorder-consumer";

// The placeholder is the literal string the production redactor emits
// in place of a resolved secret. We script the harness to put this
// directly in `args_redacted` so the recorder asserts the payload is
// passed through the worker's read path verbatim.
const SECRET_PLACEHOLDER = "${secrets.fly.api_token}";

const SCRIPT =
    \\{"frames":[
    \\  {"kind":"tool_call_started","name":"http.request","args_redacted":"{\"token\":\"${secrets.fly.api_token}\"}"}
    \\],"result":{"content":"ok","exit_ok":true}}
;

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *@import("pg").Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

test "integration: RpcRecorder captures secret-placeholder bytes from executor RPC" {
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
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, helpers.ZOMBIE_NAME, helpers.ZOMBIE_CONFIG_JSON, helpers.ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, EMPTY_CONTEXT_JSON);

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);
    defer cleanupZombieEventsRows(db_ctx.conn);

    try redis_zombie.ensureZombieConsumerGroup(&redis, TEST_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .script_json = SCRIPT });
    defer harness.deinit();

    var recorder = RpcRecorder.init(ALLOC);
    defer recorder.deinit();
    recorder.install();
    defer recorder.uninstall();

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
    const next_consec = writepath.run(ALLOC, &session, &evt, cfg, &consec);
    try std.testing.expectEqual(@as(u32, 0), next_consec);

    // The HELLO frame, the StartStage progress frame carrying the
    // tool_call_started, and the terminal response have all flowed
    // through readFrameFromFd. The placeholder bytes must appear in
    // the captured stream verbatim.
    try std.testing.expect(recorder.contains(SECRET_PLACEHOLDER));
}
