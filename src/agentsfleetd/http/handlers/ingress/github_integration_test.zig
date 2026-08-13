//! End-to-end GitHub App ingress proof through the real router, vault,
//! connector-install index, fleet configs, grant table, and Redis streams.

const std = @import("std");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const fixtures = @import("../../../db/test_fixtures_app_ingress.zig");
const base_fixtures = @import("../../../db/test_fixtures.zig");
const ec = @import("../../../errors/error_registry.zig");
const whc = @import("../../../fleet_runtime/webhook_constants.zig");
const hs = @import("hmac_sig");
const verifier = @import("../../../fleet_runtime/webhook_verify.zig");
const redis_fleet = @import("../../../queue/redis_fleet.zig");
const queue_consts = @import("../../../queue/constants.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");
const common_c = @import("common");
const repair_branch = @import("../../../git/repair_branch.zig");
const dispatcher = @import("../../../fleet/repair_verification_dispatcher.zig");
const repair_verification_queue = @import("../../../queue/redis_repair_verification.zig");
const repair_evidence = @import("../../../state/repair_evidence.zig");
const repair_verifications = @import("../../../state/repair_verifications.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

const PATH = "/v1/ingress/github";
const NON_GITHUB_PATH = "/v1/ingress/linear";
const SECRET = "github-app-ingress-test-secret";
const REPOSITORY = "agentsfleet/agentsfleet";
const REPOSITORY_MIXED_CASE = "AgentsFleet/AgentsFleet";
const DEDUP_NAMESPACE = "gh";
const DEDUP_KEY_BUF_LEN = 256;
const CONCURRENT_REQUEST_COUNT = 100;
const CONCURRENT_DEPLOYMENT_ID_BASE = 100_000;
const CONCURRENT_STATUS_ID_BASE = 200_000;
const MARKER_CASE_MATCH = "case-match";
const DELIVERY_CASE_MATCH = "delivery-case-match";
const EVENT_PULL_REQUEST = "pull_request";
const EVENT_DEPLOYMENT_STATUS = "deployment_status";
const DELIVERY_CONCURRENT_MERGE = "delivery-concurrent-merge";
const DELIVERY_CONCURRENT_PRODUCTION = "delivery-concurrent-production";
/// Two server-admitted requests are enough to disprove global serialization;
/// higher peaks depend on host scheduler load and are not a correctness rule.
const MIN_PEAK_IN_FLIGHT = 2;
const FANOUT_BASE_COUNT = 2;
const FANOUT_LIMIT = 100;
const REPAIR_GATE_ID = "0195c102-5040-7000-8000-000000000040";
const REPAIR_EVENT_ID = "evt-app-ingress-repair";
const AMBIGUOUS_REPAIR_LINK_ID = "0195c102-5043-7000-8000-000000000043";
const CONCURRENT_REPAIR_GATE_ID = "0195c102-5044-7000-8000-000000000044";
const CONCURRENT_REPAIR_EVENT_ID = "evt-app-ingress-repair-concurrent";
const CLEANUP_CLAIM_TOKEN = "0195c102-5061-7000-8000-000000000061";
const STALLED_CLAIM_TOKEN = "0195c102-5062-7000-8000-000000000062";
const REPAIR_BRANCH = repair_branch.fromGateId(REPAIR_GATE_ID) catch @panic("fixed repair gate identifier must encode");
const CONCURRENT_REPAIR_BRANCH = repair_branch.fromGateId(CONCURRENT_REPAIR_GATE_ID) catch @panic("fixed concurrent repair gate identifier must encode");
const REPAIR_BINDING = "{\"repositories\":[\"agentsfleet/agentsfleet\"],\"access\":\"write\",\"base\":\"main\"}";
const VERIFIER_FLEET_ID = "0195c102-5041-7000-8000-000000000041";
const VERIFIER_GRANT_ID = "0195c102-5042-7000-8000-000000000042";
const SECOND_VERIFIER_FLEET_ID = "0195c102-5051-7000-8000-000000000051";
const SECOND_VERIFIER_GRANT_ID = "0195c102-5052-7000-8000-000000000052";
const OTHER_WORKSPACE_REPAIR_FLEET_ID = "0195c102-5053-7000-8000-000000000053";
const OTHER_WORKSPACE_REPAIR_LINK_ID = "0195c102-5054-7000-8000-000000000054";
const DEPLOYMENT_STATUS_ID = "8401";
const SUCCESS_STATUS_MERGE_FIRST_ID = "8404";
const SUCCESS_STATUS_AFTER_FAILURE_ID = "8405";
const MISMATCH_STATUS_ID = "8408";
const MERGED_COMMIT_SHA = "exact-repair-merge-commit";
const MISMATCHED_COMMIT_SHA = "different-repair-merge-commit";
const CONCURRENT_MERGED_COMMIT_SHA = "concurrent-exact-repair-merge-commit";
const CORRELATION_LOCK_WAITERS = 2;
const CORRELATION_LOCK_WAIT_TIMEOUT_MS = 2_000;
const CORRELATION_LOCK_POLL_NS = 5 * std.time.ns_per_ms;
const CORRELATION_LOCK_POLL_ATTEMPTS = CORRELATION_LOCK_WAIT_TIMEOUT_MS * std.time.ns_per_ms / CORRELATION_LOCK_POLL_NS;
const LOG_FIXTURE_CLEANUP_FAILED = "repair fixture cleanup failed: {s}";
const CONFIG_PULL =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["pull_request"],"repositories":["agentsfleet/agentsfleet"]}],"tools":[],"budget":{"daily_dollars":1}}}
;
const CONFIG_VERIFIER =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["repair_production_result"],"repositories":["agentsfleet/agentsfleet"]}],"tools":[],"budget":{"daily_dollars":1},"repositories":["agentsfleet/agentsfleet"],"repository_access":"read"}}
;

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn postSigned(h: *TestHarness, body: []const u8, event: []const u8, delivery: []const u8, secret: []const u8) !harness_mod.Response {
    const config = verifier.GITHUB;
    const ingress = config.ingress.?;
    const mac = hs.computeMac(secret, &.{body});
    var signature_buf: ["sha256=".len + hs.MAC_LEN * 2]u8 = undefined;
    const signature = hs.encodeMacHex(&signature_buf, config.prefix, mac);
    var request = h.post(PATH);
    request = try request.header(config.sig_header, signature);
    request = try request.header(ingress.event_header, event);
    request = try request.header(ingress.delivery_header, delivery);
    return request.rawBody(body).send();
}

fn pullRequestBody(alloc: std.mem.Allocator, installation: []const u8, repository: []const u8, marker: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{{\"action\":\"opened\",\"delivery_marker\":\"{s}\",\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"pull_request\":{{\"number\":42,\"title\":\"Review this\",\"state\":\"open\",\"draft\":false,\"user\":{{\"login\":\"indy\"}},\"head\":{{\"ref\":\"fix\",\"sha\":\"abc123\"}},\"base\":{{\"ref\":\"main\"}}}}}}", .{ marker, installation, repository });
}

fn workflowBody(conclusion: []const u8) ![]const u8 {
    return std.fmt.allocPrint(testing.allocator, "{{\"action\":\"completed\",\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"workflow_run\":{{\"id\":7,\"conclusion\":\"{s}\",\"html_url\":\"https://example.test/run/7\"}}}}", .{ fixtures.INSTALLATION_ID, REPOSITORY, conclusion });
}

fn repairPullRequestBody(alloc: std.mem.Allocator) ![]const u8 {
    return repairPullRequestBodyFor(alloc, &REPAIR_BRANCH, 88);
}

fn repairPullRequestBodyFor(alloc: std.mem.Allocator, branch: []const u8, number: i64) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"action\":\"opened\",\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"pull_request\":{{\"number\":{d},\"html_url\":\"https://github.com/{s}/pull/{d}\",\"user\":{{\"login\":\"agentsfleet-test[bot]\"}},\"head\":{{\"ref\":\"{s}\",\"repo\":{{\"full_name\":\"{s}\",\"fork\":false}}}},\"base\":{{\"ref\":\"main\",\"repo\":{{\"full_name\":\"{s}\"}}}}}}}}",
        .{ fixtures.INSTALLATION_ID, REPOSITORY, number, REPOSITORY, number, branch, REPOSITORY, REPOSITORY },
    );
}

fn repairMergedBody(alloc: std.mem.Allocator) ![]const u8 {
    return repairMergedBodyFor(alloc, &REPAIR_BRANCH, 88, MERGED_COMMIT_SHA);
}

fn repairMergedBodyFor(alloc: std.mem.Allocator, branch: []const u8, number: i64, sha: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"action\":\"closed\",\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"pull_request\":{{\"number\":{d},\"html_url\":\"https://github.com/{s}/pull/{d}\",\"merged\":true,\"merge_commit_sha\":\"{s}\",\"merged_at\":\"2026-08-10T12:00:00Z\",\"user\":{{\"login\":\"agentsfleet-test[bot]\"}},\"head\":{{\"ref\":\"{s}\",\"repo\":{{\"full_name\":\"{s}\",\"fork\":false}}}},\"base\":{{\"ref\":\"main\",\"repo\":{{\"full_name\":\"{s}\"}}}}}}}}",
        .{ fixtures.INSTALLATION_ID, REPOSITORY, number, REPOSITORY, number, sha, branch, REPOSITORY, REPOSITORY },
    );
}

fn productionBody(alloc: std.mem.Allocator, status_id: []const u8, commit_sha: []const u8, environment: []const u8) ![]const u8 {
    return productionBodyWithIds(alloc, status_id, status_id, commit_sha, environment);
}

fn productionBodyWithIds(alloc: std.mem.Allocator, deployment_id: []const u8, status_id: []const u8, commit_sha: []const u8, environment: []const u8) ![]const u8 {
    return productionBodyWithState(alloc, deployment_id, status_id, commit_sha, environment, "success");
}

fn productionBodyWithState(alloc: std.mem.Allocator, deployment_id: []const u8, status_id: []const u8, commit_sha: []const u8, environment: []const u8, state: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"deployment\":{{\"id\":{s},\"sha\":\"{s}\"}},\"deployment_status\":{{\"id\":{s},\"environment\":\"{s}\",\"state\":\"{s}\",\"updated_at\":\"2026-08-10T12:00:00Z\"}}}}",
        .{ fixtures.INSTALLATION_ID, REPOSITORY, deployment_id, commit_sha, status_id, environment, state },
    );
}

fn concurrentProductionBody(alloc: std.mem.Allocator, index: usize) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"installation\":{{\"id\":{s}}},\"repository\":{{\"full_name\":\"{s}\"}},\"deployment\":{{\"id\":{d},\"sha\":\"parallel-production-commit-{d}\"}},\"deployment_status\":{{\"id\":{d},\"environment\":\"production\",\"state\":\"success\",\"updated_at\":\"2026-08-10T12:00:00Z\"}}}}",
        .{ fixtures.INSTALLATION_ID, REPOSITORY, index + CONCURRENT_DEPLOYMENT_ID_BASE, index, index + CONCURRENT_STATUS_ID_BASE },
    );
}

fn purgeRepairFixture(conn: anytype) void {
    _ = conn.exec("BEGIN", .{}) catch return;
    _ = conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch |err| logCleanupFailure(err);
        return;
    };
    _ = conn.exec("DELETE FROM core.repair_run_results WHERE fleet_id = $1::uuid", .{fixtures.FLEET_PULL_ONE}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.repair_pr_links WHERE id = $1::uuid", .{AMBIGUOUS_REPAIR_LINK_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.repair_pr_links WHERE fleet_id = $1::uuid", .{fixtures.FLEET_PULL_ONE}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleet_approval_gates WHERE id = $1::uuid", .{CONCURRENT_REPAIR_GATE_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleet_approval_gates WHERE id = $1::uuid", .{REPAIR_GATE_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2", .{ fixtures.FLEET_PULL_ONE, CONCURRENT_REPAIR_EVENT_ID }) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2", .{ fixtures.FLEET_PULL_ONE, REPAIR_EVENT_ID }) catch |err| logCleanupFailure(err);
    _ = conn.exec("COMMIT", .{}) catch |err| logCleanupFailure(err);
}

fn seedRepairEventAndGate(conn: anytype, gate_id: []const u8, event_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, response_text, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'webhook:github', 'webhook', 'received',
        \\        '{"symptom":"latency"}'::jsonb, 'Latency began immediately after deploy 17.', 1, 1)
    , .{ fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, event_id });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   resolved_by, status, detail, created_at, updated_at, event_id,
        \\   stated_binding, spend_count, spend_ceiling)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'repair-production-result', 'github', 'write',
        \\        $4, '', '{}'::jsonb, '', 9999999999999,
        \\        'indy', 'approved', '', 1, 1, $5, $6::jsonb, 0, $7)
    , .{ gate_id, fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, gate_constants.GATE_KIND_REPOSITORY_WRITE, event_id, REPAIR_BINDING, gate_constants.REPOSITORY_WRITE_SPEND_CEILING });
}

fn seedVerifier(conn: anytype) !void {
    try seedVerifierWith(conn, VERIFIER_FLEET_ID, VERIFIER_GRANT_ID, "incident-verifier-test");
}

fn seedSecondVerifier(conn: anytype) !void {
    try seedVerifierWith(conn, SECOND_VERIFIER_FLEET_ID, SECOND_VERIFIER_GRANT_ID, "incident-verifier-second-test");
}

fn seedVerifierWith(conn: anytype, fleet_id: []const u8, grant_id: []const u8, name: []const u8) !void {
    const now = @import("common").clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)" ++
            " VALUES ($1::uuid, $2::uuid, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2::uuid)," ++
            " $3, '# verifier', $4::jsonb, 'active', $5, $5)",
        .{ fleet_id, fixtures.WORKSPACE_ID, name, CONFIG_VERIFIER, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.integration_grants (id, fleet_id, service, status, created_at, requested_reason, approved_at)" ++
            " VALUES ($1::uuid, $2::uuid, 'github', 'approved', $3, 'repair verification test', $3)",
        .{ grant_id, fleet_id, now },
    );
}

fn purgeVerifier(conn: anytype) void {
    _ = conn.exec("BEGIN", .{}) catch return;
    _ = conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch |err| logCleanupFailure(err);
        return;
    };
    _ = conn.exec("DELETE FROM core.repair_verifications WHERE verifier_fleet_id = $1::uuid OR verifier_fleet_id = $2::uuid", .{ VERIFIER_FLEET_ID, SECOND_VERIFIER_FLEET_ID }) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.repair_production_results WHERE workspace_id = $1::uuid", .{fixtures.WORKSPACE_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid OR id = $2::uuid", .{ VERIFIER_FLEET_ID, SECOND_VERIFIER_FLEET_ID }) catch |err| logCleanupFailure(err);
    _ = conn.exec("COMMIT", .{}) catch |err| logCleanupFailure(err);
}

fn seedOtherWorkspaceRepair(conn: anytype) !void {
    const now = @import("common").clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)" ++
            " VALUES ($1::uuid, $2::uuid, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2::uuid)," ++
            " 'other-workspace-repair', '# repair', $3::jsonb, 'active', $4, $4)",
        .{ OTHER_WORKSPACE_REPAIR_FLEET_ID, fixtures.ADMIN_WORKSPACE_ID, CONFIG_PULL, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.repair_pr_links " ++
            "(id, workspace_id, fleet_id, event_id, repository, branch, pr_number, " ++
            "pr_url, deploy_status, merged_commit_sha, merged_at, created_at) " ++
            "VALUES ($1::uuid, $2::uuid, $3::uuid, 'other-workspace-repair-event', $4, " ++
            "'agentsfleet-repair/other-workspace', 99, " ++
            "'https://github.com/agentsfleet/agentsfleet/pull/99', " ++
            "'pending', $5, 1, 1)",
        .{ OTHER_WORKSPACE_REPAIR_LINK_ID, fixtures.ADMIN_WORKSPACE_ID, OTHER_WORKSPACE_REPAIR_FLEET_ID, REPOSITORY, MERGED_COMMIT_SHA },
    );
}

fn purgeOtherWorkspaceRepair(conn: anytype) void {
    _ = conn.exec("BEGIN", .{}) catch return;
    _ = conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch |err| logCleanupFailure(err);
        return;
    };
    _ = conn.exec("DELETE FROM core.repair_pr_links WHERE id = $1::uuid", .{OTHER_WORKSPACE_REPAIR_LINK_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{OTHER_WORKSPACE_REPAIR_FLEET_ID}) catch |err| logCleanupFailure(err);
    _ = conn.exec("COMMIT", .{}) catch |err| logCleanupFailure(err);
}

fn logCleanupFailure(err: anyerror) void {
    std.log.warn(LOG_FIXTURE_CLEANUP_FAILED, .{ .err = @errorName(err) });
}

fn countRows(conn: anytype, query: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(query, .{}));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn expectVerificationState(conn: anytype, verification_id: []const u8, event_id: []const u8, cleanup_recorded: bool) !void {
    var query = PgQuery.from(try conn.query(
        "SELECT verifier_event_id, redis_once_key_cleared_at FROM core.repair_verifications WHERE id = $1::uuid",
        .{verification_id},
    ));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings(event_id, try row.get([]const u8, 0));
    try testing.expectEqual(cleanup_recorded, (try row.get(?i64, 1)) != null);
}

fn claimVerification(conn: anytype, verification_id: []const u8, claim_token: []const u8, claimed_at: i64) !void {
    const affected = try conn.exec(
        \\UPDATE core.repair_verifications
        \\SET dispatch_claim_token = $2::uuid, dispatch_claimed_at = $3,
        \\    dispatch_attempts = dispatch_attempts + 1, updated_at = $3
        \\WHERE id = $1::uuid AND verifier_event_id IS NULL
    , .{ verification_id, claim_token, claimed_at });
    try testing.expectEqual(@as(i64, 1), affected orelse 0);
}

fn expectSuccessfulVerificationMatrix(conn: anytype) !void {
    const Expected = struct { status_id: []const u8, verifier_fleet_id: []const u8 };
    const expected = [_]Expected{
        .{ .status_id = SUCCESS_STATUS_MERGE_FIRST_ID, .verifier_fleet_id = VERIFIER_FLEET_ID },
        .{ .status_id = SUCCESS_STATUS_MERGE_FIRST_ID, .verifier_fleet_id = SECOND_VERIFIER_FLEET_ID },
        .{ .status_id = SUCCESS_STATUS_AFTER_FAILURE_ID, .verifier_fleet_id = VERIFIER_FLEET_ID },
        .{ .status_id = SUCCESS_STATUS_AFTER_FAILURE_ID, .verifier_fleet_id = SECOND_VERIFIER_FLEET_ID },
    };
    var query = PgQuery.from(try conn.query(
        \\SELECT p.provider_status_id, v.verifier_fleet_id::text, v.verifier_event_id
        \\FROM core.repair_verifications v
        \\JOIN core.repair_production_results p ON p.id = v.production_result_id
        \\WHERE p.workspace_id = $1::uuid AND p.provider = $2
        \\ORDER BY p.provider_status_id, v.verifier_fleet_id
    , .{ fixtures.WORKSPACE_ID, repair_evidence.GITHUB_PROVIDER }));
    defer query.deinit();
    for (expected) |item| {
        const row = try query.next() orelse return error.TestUnexpectedResult;
        try testing.expectEqualStrings(item.status_id, try row.get([]const u8, 0));
        try testing.expectEqualStrings(item.verifier_fleet_id, try row.get([]const u8, 1));
        try testing.expect((try row.get(?[]const u8, 2)) != null);
    }
    try testing.expect((try query.next()) == null);
    query.drain();
}

fn waitForCorrelationLockWaiters(conn: anytype) !void {
    for (0..CORRELATION_LOCK_POLL_ATTEMPTS) |_| {
        const waiters = try countRows(conn,
            \\SELECT count(*)
            \\FROM pg_locks
            \\WHERE locktype = 'advisory' AND granted = false
        );
        if (waiters >= CORRELATION_LOCK_WAITERS) return;
        common_c.sleepNanos(CORRELATION_LOCK_POLL_NS);
    }
    return error.CorrelationLockWaitersNotObserved;
}

fn waitForProductionResultCount(conn: anytype, expected: i64) !void {
    for (0..CORRELATION_LOCK_POLL_ATTEMPTS) |_| {
        if (try countRows(conn, "SELECT count(*) FROM core.repair_production_results") == expected) return;
        common_c.sleepNanos(CORRELATION_LOCK_POLL_NS);
    }
    return error.ProductionResultCountNotObserved;
}

fn streamLen(h: *TestHarness, fleet_id: []const u8) !i64 {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var response = try h.queue.command(&.{ "XLEN", key });
    defer response.deinit(testing.allocator);
    return switch (response) {
        .integer => |count| count,
        else => error.UnexpectedRedisResponse,
    };
}

/// Purge the whole Redis footprint — stream AND readiness mark — for every
/// fleet this suite delivers to. Ingress marks `fleet:ready` on each accepted
/// delivery, and that hash is ONE key shared by every suite in the binary: a
/// stream-only DEL here once left ~100 marks squatting in the bounded
/// randomized peek sample, crowding sibling suites' fleets out of their own
/// lease polls for the rest of the run.
fn clearStreams(h: *TestHarness) void {
    const fleets = [_][]const u8{ fixtures.FLEET_PULL_ONE, fixtures.FLEET_PULL_TWO, fixtures.FLEET_WORKFLOW, fixtures.FLEET_WRONG_REPO, fixtures.FLEET_NO_REPOS, fixtures.FLEET_NO_GRANT };
    for (fleets) |fleet_id| {
        redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err| std.log.warn("App ingress redis cleanup ignored: {s}", .{@errorName(err)});
    }
}

fn clearReplaySlots(h: *TestHarness) void {
    const fleets = [_][]const u8{ fixtures.FLEET_PULL_ONE, fixtures.FLEET_PULL_TWO, fixtures.FLEET_WORKFLOW };
    for (fleets) |fleet_id| {
        var pattern_buf: [DEDUP_KEY_BUF_LEN]u8 = undefined;
        const pattern = std.fmt.bufPrint(&pattern_buf, "{s}{s}:{s}:*", .{ whc.WEBHOOK_DEDUP_KEY_PREFIX, fleet_id, DEDUP_NAMESPACE }) catch continue;
        var response = h.queue.command(&.{ "KEYS", pattern }) catch continue;
        defer response.deinit(h.queue.alloc);
        const keys = response.array orelse continue;
        for (keys) |key_value| {
            const key = switch (key_value) {
                .bulk => |value| value orelse continue,
                else => continue,
            };
            h.queue.del(key) catch |err| std.log.warn("App ingress replay cleanup ignored: {s}", .{@errorName(err)});
        }
    }
}

fn setStreamFault(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var deleted = try h.queue.commandAllowError(&.{ "DEL", key });
    deleted.deinit(h.queue.alloc);
    var fault = try h.queue.commandAllowError(&.{ "SET", key, "fault" });
    fault.deinit(h.queue.alloc);
}

/// Mid-test fault RESET only (the stream must be recreatable by the retry) —
/// end-of-test cleanup goes through `purgeFleetRedisState` instead, which also
/// drops the readiness mark.
fn clearStream(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var deleted = try h.queue.commandAllowError(&.{ "DEL", key });
    deleted.deinit(h.queue.alloc);
}

fn fanoutId(buf: []u8, index: usize, grant: bool) ![]const u8 {
    return std.fmt.bufPrint(buf, "0195c102-{s}-7000-8000-{d:0>12}", .{ if (grant) "6100" else "6000", index });
}

fn seedFanoutFleet(conn: anytype, index: usize) !void {
    var fleet_buf: [36]u8 = undefined;
    var grant_buf: [36]u8 = undefined;
    var name_buf: [32]u8 = undefined;
    const fleet_id = try fanoutId(&fleet_buf, index, false);
    const grant_id = try fanoutId(&grant_buf, index, true);
    const name = try std.fmt.bufPrint(&name_buf, "app-fanout-{d}", .{index});
    const now = @import("common").clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)" ++
            " VALUES ($1::uuid, $2::uuid, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2::uuid)," ++
            " $3, '# test fleet', $4::jsonb, 'active', $5, $5)",
        .{ fleet_id, fixtures.WORKSPACE_ID, name, CONFIG_PULL, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.integration_grants (id, fleet_id, service, status, created_at, requested_reason, approved_at) VALUES ($1::uuid, $2::uuid, 'github', 'approved', $3, 'fanout boundary test', $3)",
        .{ grant_id, fleet_id, now },
    );
}

fn clearFanoutStreams(h: *TestHarness, count: usize) void {
    for (0..count) |index| {
        var id_buf: [36]u8 = undefined;
        const fleet_id = fanoutId(&id_buf, index, false) catch continue;
        redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err| std.log.warn("App ingress fanout redis cleanup ignored: {s}", .{@errorName(err)});
    }
}

test "integration: test_ingress_resolves_gate_event_owner" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    purgeRepairFixture(conn);
    defer purgeRepairFixture(conn);
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    clearStreams(h);
    defer clearStreams(h);
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;
    h.ctx.github_app_slug = "agentsfleet-test";
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'webhook:github', 'webhook', 'received',
        \\        '{}'::jsonb, 1, 1)
    , .{ fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, REPAIR_EVENT_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   resolved_by, status, detail, created_at, updated_at, event_id,
        \\   stated_binding, spend_count, spend_ceiling)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'repair-app-ingress', 'github', 'write',
        \\        $4, '', '{}'::jsonb, '', 9999999999999,
        \\        'indy', 'approved', '', 1, 1, $5, $6::jsonb, 0, $7)
    , .{ REPAIR_GATE_ID, fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, gate_constants.GATE_KIND_REPOSITORY_WRITE, REPAIR_EVENT_ID, REPAIR_BINDING, gate_constants.REPOSITORY_WRITE_SPEND_CEILING });

    const body = try repairPullRequestBody(testing.allocator);
    defer testing.allocator.free(body);
    const response = try postSigned(h, body, "pull_request", "delivery-repair-link", SECRET);
    defer response.deinit();
    try response.expectStatus(.ok);
    try testing.expect(response.bodyContains(REPAIR_EVENT_ID));
    var q = PgQuery.from(try conn.query(
        \\SELECT fleet_id::text, event_id, repository
        \\FROM core.repair_pr_links
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ fixtures.FLEET_PULL_ONE, REPAIR_EVENT_ID }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings(fixtures.FLEET_PULL_ONE, try row.get([]const u8, 0));
    try testing.expectEqualStrings(REPAIR_EVENT_ID, try row.get([]const u8, 1));
    try testing.expectEqualStrings(REPOSITORY, try row.get([]const u8, 2));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_PULL_TWO));
}

test "integration: production results reconcile in either arrival order and emit verifier once" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    purgeVerifier(conn);
    purgeRepairFixture(conn);
    purgeOtherWorkspaceRepair(conn);
    defer {
        purgeVerifier(conn);
        purgeRepairFixture(conn);
        purgeOtherWorkspaceRepair(conn);
    }
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    clearStreams(h);
    defer clearStreams(h);
    try redis_fleet.purgeFleetRedisState(&h.queue, VERIFIER_FLEET_ID);
    defer redis_fleet.purgeFleetRedisState(&h.queue, VERIFIER_FLEET_ID) catch |err| logCleanupFailure(err);
    try redis_fleet.purgeFleetRedisState(&h.queue, SECOND_VERIFIER_FLEET_ID);
    defer redis_fleet.purgeFleetRedisState(&h.queue, SECOND_VERIFIER_FLEET_ID) catch |err| logCleanupFailure(err);
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;
    h.ctx.github_app_slug = "agentsfleet-test";
    try seedVerifier(conn);
    try seedSecondVerifier(conn);
    try seedOtherWorkspaceRepair(conn);
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, response_text, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'webhook:github', 'webhook', 'received',
        \\        '{"symptom":"latency"}'::jsonb, $4, 1, 1)
    , .{ fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, REPAIR_EVENT_ID, "Latency began immediately after deploy 17." });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   resolved_by, status, detail, created_at, updated_at, event_id,
        \\   stated_binding, spend_count, spend_ceiling)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'repair-production-result', 'github', 'write',
        \\        $4, '', '{}'::jsonb, '', 9999999999999,
        \\        'indy', 'approved', '', 1, 1, $5, $6::jsonb, 0, $7)
    , .{ REPAIR_GATE_ID, fixtures.FLEET_PULL_ONE, fixtures.WORKSPACE_ID, gate_constants.GATE_KIND_REPOSITORY_WRITE, REPAIR_EVENT_ID, REPAIR_BINDING, gate_constants.REPOSITORY_WRITE_SPEND_CEILING });

    const result_first = try productionBodyWithState(testing.allocator, DEPLOYMENT_STATUS_ID, DEPLOYMENT_STATUS_ID, MERGED_COMMIT_SHA, "production", "failure");
    defer testing.allocator.free(result_first);
    const result_response = try postSigned(h, result_first, "deployment_status", "delivery-production-first", SECRET);
    defer result_response.deinit();
    try result_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const opened = try repairPullRequestBody(testing.allocator);
    defer testing.allocator.free(opened);
    const opened_response = try postSigned(h, opened, "pull_request", "delivery-repair-open", SECRET);
    defer opened_response.deinit();
    try opened_response.expectStatus(.ok);
    const merged = try repairMergedBody(testing.allocator);
    defer testing.allocator.free(merged);
    const merged_response = try postSigned(h, merged, "pull_request", "delivery-repair-merged", SECRET);
    defer merged_response.deinit();
    try merged_response.expectStatus(.ok);
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const replay_response = try postSigned(h, result_first, "deployment_status", "delivery-production-replay", SECRET);
    defer replay_response.deinit();
    try replay_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const second_status = try productionBodyWithIds(testing.allocator, DEPLOYMENT_STATUS_ID, SUCCESS_STATUS_AFTER_FAILURE_ID, MERGED_COMMIT_SHA, "production");
    defer testing.allocator.free(second_status);
    const second_status_response = try postSigned(h, second_status, "deployment_status", "delivery-production-second-status", SECRET);
    defer second_status_response.deinit();
    try second_status_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 2), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 2), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const merge_first = try productionBody(testing.allocator, SUCCESS_STATUS_MERGE_FIRST_ID, MERGED_COMMIT_SHA, "production");
    defer testing.allocator.free(merge_first);
    const merge_first_response = try postSigned(h, merge_first, "deployment_status", "delivery-production-after-merge", SECRET);
    defer merge_first_response.deinit();
    try merge_first_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 3), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const mismatch = try productionBody(testing.allocator, MISMATCH_STATUS_ID, MISMATCHED_COMMIT_SHA, "production");
    defer testing.allocator.free(mismatch);
    const mismatch_response = try postSigned(h, mismatch, "deployment_status", "delivery-production-mismatch", SECRET);
    defer mismatch_response.deinit();
    try mismatch_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));

    const preview = try productionBody(testing.allocator, "8402", MERGED_COMMIT_SHA, "preview");
    defer testing.allocator.free(preview);
    const preview_response = try postSigned(h, preview, "deployment_status", "delivery-preview", SECRET);
    defer preview_response.deinit();
    try preview_response.expectStatus(.ok);
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));

    const pending = try productionBodyWithState(testing.allocator, "8406", "8407", MERGED_COMMIT_SHA, "production", "pending");
    defer testing.allocator.free(pending);
    const pending_response = try postSigned(h, pending, "deployment_status", "delivery-production-pending", SECRET);
    defer pending_response.deinit();
    try pending_response.expectStatus(.ok);
    try testing.expect(pending_response.bodyContains("non_terminal_deployment_state"));
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));

    var due_query = PgQuery.from(try conn.query(
        "SELECT id::text, verifier_fleet_id::text, verify_after FROM core.repair_verifications ORDER BY id",
        .{},
    ));
    defer due_query.deinit();
    const cleanup_row = try due_query.next() orelse return error.TestUnexpectedResult;
    const cleanup_verification_id = try testing.allocator.dupe(u8, try cleanup_row.get([]const u8, 0));
    defer testing.allocator.free(cleanup_verification_id);
    const cleanup_verifier_fleet_id = try testing.allocator.dupe(u8, try cleanup_row.get([]const u8, 1));
    defer testing.allocator.free(cleanup_verifier_fleet_id);
    const verify_after = try cleanup_row.get(i64, 2);
    const due_row = try due_query.next() orelse return error.TestUnexpectedResult;
    const verification_id = try testing.allocator.dupe(u8, try due_row.get([]const u8, 0));
    defer testing.allocator.free(verification_id);
    const verifier_fleet_id = try testing.allocator.dupe(u8, try due_row.get([]const u8, 1));
    defer testing.allocator.free(verifier_fleet_id);
    try testing.expectEqual(verify_after, try due_row.get(i64, 2));
    due_query.drain();
    const before = try dispatcher.dispatchOnce(h.pool, &h.queue, testing.allocator, verify_after - 1);
    try testing.expectEqual(@as(usize, 0), before.due);

    // Simulate a crash after durable completion but before deleting the Redis
    // once-key. The next sweep must finish cleanup without appending an event.
    const cleanup_enqueue = try repair_verification_queue.xaddOnce(&h.queue, cleanup_verification_id, .{
        .event_id = "",
        .fleet_id = cleanup_verifier_fleet_id,
        .workspace_id = fixtures.WORKSPACE_ID,
        .actor = repair_verifications.VERIFIER_EVENT_ACTOR,
        .event_type = .webhook,
        .request_json = "{\"source\":\"pre-cleanup-crash\"}",
        .created_at = verify_after,
    });
    defer h.queue.alloc.free(cleanup_enqueue.event_id);
    try testing.expect(!cleanup_enqueue.replayed);
    try claimVerification(conn, cleanup_verification_id, CLEANUP_CLAIM_TOKEN, verify_after);
    try testing.expect(try repair_verifications.complete(conn, cleanup_verification_id, CLEANUP_CLAIM_TOKEN, cleanup_enqueue.event_id, verify_after));
    try expectVerificationState(conn, cleanup_verification_id, cleanup_enqueue.event_id, false);

    // This is the crash boundary: Redis accepted the event but PostgreSQL has
    // not recorded it. The dispatcher must replay the same stream identifier,
    // complete the durable row, and never append a second event.
    const pre_completion_enqueue = try repair_verification_queue.xaddOnce(&h.queue, verification_id, .{
        .event_id = "",
        .fleet_id = verifier_fleet_id,
        .workspace_id = fixtures.WORKSPACE_ID,
        .actor = repair_verifications.VERIFIER_EVENT_ACTOR,
        .event_type = .webhook,
        .request_json = "{\"source\":\"pre-completion-crash\"}",
        .created_at = verify_after,
    });
    defer h.queue.alloc.free(pre_completion_enqueue.event_id);
    try testing.expect(!pre_completion_enqueue.replayed);

    try claimVerification(conn, verification_id, STALLED_CLAIM_TOKEN, verify_after);
    const blocked = try dispatcher.dispatchOnce(h.pool, &h.queue, testing.allocator, verify_after);
    try testing.expectEqual(@as(usize, 2), blocked.due);
    try testing.expectEqual(@as(usize, 2), blocked.completed);
    try testing.expectEqual(@as(i64, 4), (try streamLen(h, VERIFIER_FLEET_ID)) + (try streamLen(h, SECOND_VERIFIER_FLEET_ID)));
    try expectVerificationState(conn, cleanup_verification_id, cleanup_enqueue.event_id, false);

    const recovery_now = verify_after + repair_verifications.CLAIM_STALE_MS;
    const emitted = try dispatcher.dispatchOnce(h.pool, &h.queue, testing.allocator, recovery_now);
    try testing.expectEqual(@as(usize, 1), emitted.completed);
    try testing.expectEqual(@as(i64, 4), (try streamLen(h, VERIFIER_FLEET_ID)) + (try streamLen(h, SECOND_VERIFIER_FLEET_ID)));
    try expectVerificationState(conn, cleanup_verification_id, cleanup_enqueue.event_id, true);
    const replayed = try dispatcher.dispatchOnce(h.pool, &h.queue, testing.allocator, recovery_now);
    try testing.expectEqual(@as(usize, 0), replayed.due);
    try testing.expectEqual(@as(i64, 4), (try streamLen(h, VERIFIER_FLEET_ID)) + (try streamLen(h, SECOND_VERIFIER_FLEET_ID)));
    try testing.expectEqual(@as(i64, 2), try streamLen(h, VERIFIER_FLEET_ID));
    try testing.expectEqual(@as(i64, 2), try streamLen(h, SECOND_VERIFIER_FLEET_ID));
    try expectSuccessfulVerificationMatrix(conn);
    try expectVerificationState(conn, verification_id, pre_completion_enqueue.event_id, false);
    _ = try dispatcher.dispatchOnce(h.pool, &h.queue, testing.allocator, recovery_now + repair_verifications.CLAIM_STALE_MS);
    try expectVerificationState(conn, verification_id, pre_completion_enqueue.event_id, true);

    _ = try conn.exec(
        \\INSERT INTO core.repair_pr_links
        \\  (id, workspace_id, fleet_id, event_id, repository, branch, pr_number,
        \\   pr_url, deploy_status, merged_commit_sha, merged_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'ambiguous-repair-event', $4,
        \\        'agentsfleet-repair/ambiguous', 89, 'https://github.com/agentsfleet/agentsfleet/pull/89',
        \\        'pending', $5, 1, 1)
    , .{ AMBIGUOUS_REPAIR_LINK_ID, fixtures.WORKSPACE_ID, fixtures.FLEET_PULL_ONE, REPOSITORY, MERGED_COMMIT_SHA });
    const ambiguous = try productionBody(testing.allocator, "8403", MERGED_COMMIT_SHA, "production");
    defer testing.allocator.free(ambiguous);
    const ambiguous_response = try postSigned(h, ambiguous, "deployment_status", "delivery-production-ambiguous", SECRET);
    defer ambiguous_response.deinit();
    try ambiguous_response.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 4), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));
}

test "integration: concurrent merge and production correlation waits then converges" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    purgeVerifier(conn);
    purgeRepairFixture(conn);
    defer {
        purgeVerifier(conn);
        purgeRepairFixture(conn);
    }
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;
    h.ctx.github_app_slug = "agentsfleet-test";
    try seedVerifier(conn);
    try seedRepairEventAndGate(conn, CONCURRENT_REPAIR_GATE_ID, CONCURRENT_REPAIR_EVENT_ID);

    const merged = try repairMergedBodyFor(testing.allocator, &CONCURRENT_REPAIR_BRANCH, 89, CONCURRENT_MERGED_COMMIT_SHA);
    defer testing.allocator.free(merged);
    const production = try productionBody(testing.allocator, "8410", CONCURRENT_MERGED_COMMIT_SHA, "production");
    defer testing.allocator.free(production);
    _ = try conn.exec("BEGIN", .{});
    var lock_held = true;
    var threads: [2]std.Thread = undefined;
    var spawned: usize = 0;
    var joined = false;
    defer {
        if (lock_held) _ = conn.exec("ROLLBACK", .{}) catch |err| logCleanupFailure(err);
        if (!joined) for (threads[0..spawned]) |thread| thread.join();
    }
    try repair_evidence.lockCorrelation(conn, fixtures.WORKSPACE_ID, REPOSITORY, CONCURRENT_MERGED_COMMIT_SHA);

    const Worker = struct {
        fn run(
            harness: *TestHarness,
            body: []const u8,
            event: []const u8,
            delivery: []const u8,
            status: *u16,
            ready_count: *std.atomic.Value(usize),
            start_gate: *std.atomic.Value(bool),
        ) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
            const response = postSigned(harness, body, event, delivery, SECRET) catch return;
            defer response.deinit();
            status.* = response.status;
        }
    };
    var statuses: [2]u16 = .{ 0, 0 };
    var ready = std.atomic.Value(usize).init(0);
    var start_gate = std.atomic.Value(bool).init(false);
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, merged, EVENT_PULL_REQUEST, DELIVERY_CONCURRENT_MERGE, &statuses[0], &ready, &start_gate });
    spawned += 1;
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, production, EVENT_DEPLOYMENT_STATUS, DELIVERY_CONCURRENT_PRODUCTION, &statuses[1], &ready, &start_gate });
    spawned += 1;
    while (ready.load(.acquire) != threads.len) std.atomic.spinLoopHint();
    start_gate.store(true, .release);
    try waitForCorrelationLockWaiters(conn);
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));
    _ = try conn.exec("ROLLBACK", .{});
    lock_held = false;
    for (threads) |thread| thread.join();
    joined = true;

    try testing.expectEqual(@as(u16, 200), statuses[0]);
    try testing.expectEqual(@as(u16, 202), statuses[1]);
    try testing.expectEqual(@as(i64, 1), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 1), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));
}

test "integration: distinct production correlations admit one hundred requests concurrently" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    purgeVerifier(conn);
    defer purgeVerifier(conn);
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;

    var bodies: [CONCURRENT_REQUEST_COUNT][]const u8 = undefined;
    var body_count: usize = 0;
    defer for (bodies[0..body_count]) |body| testing.allocator.free(body);
    var delivery_buffers: [CONCURRENT_REQUEST_COUNT][64]u8 = undefined;
    var threads: [CONCURRENT_REQUEST_COUNT]std.Thread = undefined;
    var spawned: usize = 0;
    var statuses: [CONCURRENT_REQUEST_COUNT]u16 = .{0} ** CONCURRENT_REQUEST_COUNT;
    var ready = std.atomic.Value(usize).init(0);
    var start_gate = std.atomic.Value(bool).init(false);
    var start_released = false;
    var joined = false;
    var server_peak = std.atomic.Value(u32).init(0);
    h.ctx.api_peak_in_flight_probe = &server_peak;
    defer h.ctx.api_peak_in_flight_probe = null;
    const Worker = struct {
        fn run(
            harness: *TestHarness,
            body: []const u8,
            delivery: []const u8,
            status: *u16,
            ready_count: *std.atomic.Value(usize),
            gate: *std.atomic.Value(bool),
        ) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            while (!gate.load(.acquire)) std.atomic.spinLoopHint();
            const response = postSigned(harness, body, EVENT_DEPLOYMENT_STATUS, delivery, SECRET) catch return;
            defer response.deinit();
            status.* = response.status;
        }
    };
    for (&bodies, 0..) |*body, index| {
        body.* = try concurrentProductionBody(testing.allocator, index);
        body_count += 1;
    }
    _ = try conn.exec("BEGIN", .{});
    var lock_held = true;
    defer {
        if (lock_held) _ = conn.exec("ROLLBACK", .{}) catch |err| logCleanupFailure(err);
        if (!start_released) start_gate.store(true, .release);
        if (!joined) for (threads[0..spawned]) |thread| thread.join();
    }
    try repair_evidence.lockCorrelation(conn, fixtures.WORKSPACE_ID, REPOSITORY, "parallel-production-commit-0");
    for (&threads, bodies, 0..) |*thread, body, index| {
        const delivery = try std.fmt.bufPrint(&delivery_buffers[index], "delivery-production-parallel-{d}", .{index});
        thread.* = try std.Thread.spawn(.{}, Worker.run, .{ h, body, delivery, &statuses[index], &ready, &start_gate });
        spawned += 1;
    }
    while (ready.load(.acquire) != CONCURRENT_REQUEST_COUNT) std.atomic.spinLoopHint();
    start_gate.store(true, .release);
    start_released = true;
    try waitForProductionResultCount(conn, CONCURRENT_REQUEST_COUNT - 1);
    try testing.expectEqual(@as(u16, 0), statuses[0]);
    _ = try conn.exec("ROLLBACK", .{});
    lock_held = false;
    for (threads) |thread| thread.join();
    joined = true;

    for (statuses) |status| try testing.expectEqual(@as(u16, 202), status);
    try testing.expect(server_peak.load(.acquire) >= MIN_PEAK_IN_FLIGHT);
    try testing.expectEqual(@as(i64, CONCURRENT_REQUEST_COUNT), try countRows(conn, "SELECT count(*) FROM core.repair_production_results"));
    try testing.expectEqual(@as(i64, 0), try countRows(conn, "SELECT count(*) FROM core.repair_verifications"));
}

test "integration: GitHub App ingress routes installation repository event grant and replay" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    clearStreams(h);
    clearReplaySlots(h);
    defer {
        clearStreams(h);
        clearReplaySlots(h);
    }
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;

    const ping = try postSigned(h, "{\"zen\":\"Keep it logically awesome.\"}", "ping", "delivery-ping", SECRET);
    defer ping.deinit();
    try ping.expectStatus(.ok);
    try testing.expect(ping.bodyContains("\"status\":\"pong\""));

    const pull = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY, "base");
    defer testing.allocator.free(pull);
    const accepted = try postSigned(h, pull, "pull_request", "delivery-pr-1", SECRET);
    defer accepted.deinit();
    try accepted.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_TWO));

    const changed_delivery_replay = try postSigned(h, pull, "pull_request", "delivery-pr-changed", SECRET);
    defer changed_delivery_replay.deinit();
    try changed_delivery_replay.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_TWO));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_WRONG_REPO));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_NO_REPOS));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_NO_GRANT));

    const replay = try postSigned(h, pull, "pull_request", "delivery-pr-1", SECRET);
    defer replay.deinit();
    try replay.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_TWO));

    const bad = try postSigned(h, pull, "pull_request", "delivery-pr-bad", "wrong-secret");
    defer bad.deinit();
    try bad.expectStatus(.unauthorized);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));

    const unknown = try pullRequestBody(testing.allocator, "99999999", REPOSITORY, "unknown");
    defer testing.allocator.free(unknown);
    const unmapped = try postSigned(h, unknown, "pull_request", "delivery-pr-unmapped", SECRET);
    defer unmapped.deinit();
    try unmapped.expectStatus(.ok);

    const failed_workflow = try workflowBody("failure");
    defer testing.allocator.free(failed_workflow);
    const workflow = try postSigned(h, failed_workflow, "workflow_run", "delivery-run-1", SECRET);
    defer workflow.deinit();
    try workflow.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_WORKFLOW));

    const successful_workflow = try workflowBody("success");
    defer testing.allocator.free(successful_workflow);
    const ignored = try postSigned(h, successful_workflow, "workflow_run", "delivery-run-2", SECRET);
    defer ignored.deinit();
    try ignored.expectStatus(.ok);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_WORKFLOW));

    const unsupported = try postSigned(h, "{\"installation\":{\"id\":123456},\"repository\":{\"full_name\":\"agentsfleet/agentsfleet\"}}", "issues", "delivery-unsupported", SECRET);
    defer unsupported.deinit();
    try unsupported.expectStatus(.ok);
    try testing.expectEqualStrings("{\"status\":\"ignored\"}", unsupported.body);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_WORKFLOW));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));

    // One failed queue target releases only its replay slot. Retrying the same
    // delivery fills the missing stream without duplicating the successful one.
    const partial_pull = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY, "partial");
    defer testing.allocator.free(partial_pull);
    try setStreamFault(h, fixtures.FLEET_PULL_TWO);
    const partial = try postSigned(h, partial_pull, "pull_request", "delivery-pr-partial", SECRET);
    defer partial.deinit();
    try partial.expectStatus(.internal_server_error);
    try testing.expectEqual(@as(i64, 2), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try clearStream(h, fixtures.FLEET_PULL_TWO);
    const recovered = try postSigned(h, partial_pull, "pull_request", "delivery-pr-partial", SECRET);
    defer recovered.deinit();
    try recovered.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 2), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_TWO));

    const concurrent_pull = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY, "concurrent");
    defer testing.allocator.free(concurrent_pull);
    var threads: [CONCURRENT_REQUEST_COUNT]std.Thread = undefined;
    var statuses: [CONCURRENT_REQUEST_COUNT]u16 = .{0} ** CONCURRENT_REQUEST_COUNT;
    var ready = std.atomic.Value(usize).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var server_peak = std.atomic.Value(u32).init(0);
    h.ctx.api_peak_in_flight_probe = &server_peak;
    defer h.ctx.api_peak_in_flight_probe = null;
    const Worker = struct {
        fn run(
            harness: *TestHarness,
            body: []const u8,
            status: *u16,
            ready_count: *std.atomic.Value(usize),
            start_gate: *std.atomic.Value(bool),
        ) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
            const response = postSigned(harness, body, "pull_request", "delivery-pr-concurrent", SECRET) catch return;
            defer response.deinit();
            status.* = response.status;
        }
    };
    for (&threads, 0..) |*thread, index| thread.* = try std.Thread.spawn(.{}, Worker.run, .{
        h,
        concurrent_pull,
        &statuses[index],
        &ready,
        &gate,
    });
    while (ready.load(.acquire) != CONCURRENT_REQUEST_COUNT) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (threads) |thread| thread.join();
    for (statuses) |status| try testing.expectEqual(@as(u16, 202), status);
    try testing.expect(server_peak.load(.acquire) >= MIN_PEAK_IN_FLIGHT);
    try testing.expectEqual(@as(i64, 3), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 2), try streamLen(h, fixtures.FLEET_PULL_TWO));

    for (0..FANOUT_LIMIT - FANOUT_BASE_COUNT) |index| try seedFanoutFleet(conn, index);
    defer clearFanoutStreams(h, FANOUT_LIMIT - FANOUT_BASE_COUNT + 1);
    const at_limit_body = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY, "fanout-100");
    defer testing.allocator.free(at_limit_body);
    const at_limit = try postSigned(h, at_limit_body, "pull_request", "delivery-fanout-100", SECRET);
    defer at_limit.deinit();
    try at_limit.expectStatus(.accepted);
    try testing.expect(at_limit.bodyContains("\"matched\":100"));
    var first_fanout_buf: [36]u8 = undefined;
    try testing.expectEqual(@as(i64, 1), try streamLen(h, try fanoutId(&first_fanout_buf, 0, false)));

    try seedFanoutFleet(conn, FANOUT_LIMIT - FANOUT_BASE_COUNT);
    const over_limit_body = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY, "fanout-101");
    defer testing.allocator.free(over_limit_body);
    const over_limit = try postSigned(h, over_limit_body, "pull_request", "delivery-fanout-101", SECRET);
    defer over_limit.deinit();
    try over_limit.expectStatus(.internal_server_error);
    var extra_fanout_buf: [36]u8 = undefined;
    try testing.expectEqual(@as(i64, 0), try streamLen(h, try fanoutId(&extra_fanout_buf, FANOUT_LIMIT - FANOUT_BASE_COUNT, false)));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, try fanoutId(&first_fanout_buf, 0, false)));
}

test "integration: GitHub App ingress matches repositories case-insensitively" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base_fixtures.setTestEncryptionKey();
    fixtures.cleanup(conn);
    defer fixtures.cleanup(conn);
    try fixtures.seed(testing.allocator, conn, SECRET);
    clearStreams(h);
    clearReplaySlots(h);
    defer {
        clearStreams(h);
        clearReplaySlots(h);
    }
    h.ctx.platform_admin_workspace_id = fixtures.ADMIN_WORKSPACE_ID;

    const mixed_case = try pullRequestBody(testing.allocator, fixtures.INSTALLATION_ID, REPOSITORY_MIXED_CASE, MARKER_CASE_MATCH);
    defer testing.allocator.free(mixed_case);
    const accepted = try postSigned(h, mixed_case, "pull_request", DELIVERY_CASE_MATCH, SECRET);
    defer accepted.deinit();
    try accepted.expectStatus(.accepted);
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLen(h, fixtures.FLEET_PULL_TWO));
    try testing.expectEqual(@as(i64, 0), try streamLen(h, fixtures.FLEET_WRONG_REPO));
}

test "integration: GitHub App ingress rejects non-GitHub route providers" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const rejected = try h.post(NON_GITHUB_PATH).rawBody("{}").send();
    defer rejected.deinit();
    try rejected.expectStatus(.not_found);
    try testing.expect(rejected.bodyContains(ec.ERR_CONNECTOR_UNKNOWN));
}
