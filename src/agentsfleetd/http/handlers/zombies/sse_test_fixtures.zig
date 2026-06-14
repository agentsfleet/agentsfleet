// Shared fixtures for the SSE-surface integration suites
// (sse_streaming_integration_test.zig, backpressure_integration_test.zig).
// One declaration site for the signed operator token, its JWKS, and the
// tenant/workspace rows its claims reference (RULE UFS / RULE TFX) — plus the
// seed/cleanup/publisher plumbing both suites repeat. Suite-specific zombies
// and choreography stay in each suite.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const queue_redis = @import("../../../queue/redis.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("test_sse_client.zig");
const metrics = @import("../../../observability/metrics.zig");

const IGNORED_ERROR_FMT = "ignored: {s}";

/// Bounded wait for detached stream threads to tear down after a close.
const DRAIN_MAX_ATTEMPTS: usize = 40;
const DRAIN_POLL_NS: u64 = 100 * std.time.ns_per_ms;

// Tenant/workspace ids embedded in TOKEN_OPERATOR's metadata claims — the
// path-workspace authorization matches these against the seeded rows.
pub const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
pub const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
pub const TEST_ISSUER = "https://clerk.dev.usezombie.com";
pub const TEST_AUDIENCE = "https://api.usezombie.com";
pub const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
pub const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

pub const SUBSCRIBE_SETTLE_NS: u64 = 200 * std.time.ns_per_ms;
pub const TEST_REDIS_URL_ENV = "TEST_REDIS_TLS_URL";
pub const HANDLER_REDIS_URL_ENV = "REDIS_URL_API";

pub fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

/// Skip unless both env vars resolve to a usable Redis: TEST_REDIS_TLS_URL
/// for the test-side publisher and REDIS_URL_API for the SSE handler's
/// subscriber. `make test-integration` exports both pointing at the same
/// instance.
pub fn requireRedisEnvOrSkip() !void {
    _ = common.env.testLiveValue(HANDLER_REDIS_URL_ENV) orelse return error.SkipZigTest;
    _ = common.env.testLiveValue(TEST_REDIS_URL_ENV) orelse return error.SkipZigTest;
}

/// Boot a harness wired with the operator token's JWKS/issuer/audience and
/// seed the tenant + workspace rows its claims reference. Suites seed their
/// own zombies afterwards.
pub fn startHarnessWithWorkspace(alloc: std.mem.Allocator) !*TestHarness {
    try requireRedisEnvOrSkip();

    const h = try TestHarness.start(alloc, .{
        .configureRegistry = noopRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);
    return h;
}

pub fn seedWorkspace(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'SseStreamingTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
}

pub fn seedZombie(conn: *pg.Conn, zombie_id: []const u8, name: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, '---\nname: zz\n---\ntest', '{"name":"zz"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_id, TEST_WORKSPACE_ID, name });
}

pub fn cleanupWorkspaceData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

pub fn connectPublisher(alloc: std.mem.Allocator) !queue_redis.Client {
    const tls_url = common.env.testLiveValue(TEST_REDIS_URL_ENV) orelse return error.SkipZigTest;
    return queue_redis.testing.connectFromUrl(common.globalIo(), alloc, tls_url);
}

pub fn streamPath(alloc: std.mem.Allocator, zombie_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/{s}/events/stream", .{ TEST_WORKSPACE_ID, zombie_id });
}

pub fn activityChannel(alloc: std.mem.Allocator, zombie_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "zombie:{s}:activity", .{zombie_id});
}

/// Close the client socket, then PUBLISH one sentinel frame so the stream
/// thread wakes from its subscription pop, attempts a write, hits
/// BrokenPipe, and tears down promptly. Without the publish the thread
/// would idle until its next heartbeat tick before noticing the dead
/// client, slowing every teardown by up to that interval.
pub fn closeAndWakeSubscriber(sc: *SseClient, pub_client: *queue_redis.Client, channel: []const u8) void {
    sc.closeStream();
    pub_client.publish(channel, "{\"kind\":\"drain\",\"event_id\":\"_\"}") catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    // Streams run on detached threads that free their StreamJob BEFORE
    // releasing the stream slot (LIFO defers in streamThreadMain). Waiting
    // for the gauge to hit zero keeps test teardown from racing the thread —
    // std.testing.allocator would report the in-flight job as a leak.
    // Callers close their last live stream, so zero is the settled state.
    var attempt: usize = 0;
    while (attempt < DRAIN_MAX_ATTEMPTS) : (attempt += 1) {
        if (metrics.snapshot().sse_in_flight_streams == 0) return;
        common.sleepNanos(DRAIN_POLL_NS);
    }
    std.log.warn("sse stream slot not drained after close", .{});
}
