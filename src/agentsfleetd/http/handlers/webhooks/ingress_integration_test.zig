//! End-to-end GitHub App ingress proof through the real router, vault,
//! connector-install index, fleet configs, grant table, and Redis streams.

const std = @import("std");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const fixtures = @import("../../../db/test_fixtures_app_ingress.zig");
const base_fixtures = @import("../../../db/test_fixtures.zig");
const ec = @import("../../../errors/error_registry.zig");
const hs = @import("hmac_sig");
const verifier = @import("../../../fleet_runtime/webhook_verify.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

const PATH = "/v1/ingress/github";
const SECRET = "github-app-ingress-test-secret";
const REPOSITORY = "agentsfleet/agentsfleet";
const DEDUP_NAMESPACE = "gh";
const DEDUP_KEY_BUF_LEN = 256;
const CONCURRENT_REQUEST_COUNT = 100;
/// Two server-admitted requests are enough to disprove global serialization;
/// higher peaks depend on host scheduler load and are not a correctness rule.
const MIN_PEAK_IN_FLIGHT = 2;
const FANOUT_BASE_COUNT = 2;
const FANOUT_LIMIT = 100;
const CONFIG_PULL =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["pull_request"],"repositories":["agentsfleet/agentsfleet"]}],"tools":[],"budget":{"daily_dollars":1}}}
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

fn streamLen(h: *TestHarness, fleet_id: []const u8) !i64 {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var response = try h.queue.command(&.{ "XLEN", key });
    defer response.deinit(testing.allocator);
    return switch (response) {
        .integer => |count| count,
        else => error.UnexpectedRedisResponse,
    };
}

fn clearStreams(h: *TestHarness) void {
    const fleets = [_][]const u8{ fixtures.FLEET_PULL_ONE, fixtures.FLEET_PULL_TWO, fixtures.FLEET_WORKFLOW, fixtures.FLEET_WRONG_REPO, fixtures.FLEET_NO_REPOS, fixtures.FLEET_NO_GRANT };
    for (fleets) |fleet_id| {
        var key_buf: [128]u8 = undefined;
        const key = std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id}) catch continue;
        h.queue.del(key) catch |err| std.log.warn("App ingress stream cleanup ignored: {s}", .{@errorName(err)});
    }
}

fn clearReplaySlots(h: *TestHarness) void {
    const fleets = [_][]const u8{ fixtures.FLEET_PULL_ONE, fixtures.FLEET_PULL_TWO, fixtures.FLEET_WORKFLOW };
    for (fleets) |fleet_id| {
        var pattern_buf: [DEDUP_KEY_BUF_LEN]u8 = undefined;
        const pattern = std.fmt.bufPrint(&pattern_buf, "{s}{s}:{s}:*", .{ ec.WEBHOOK_DEDUP_KEY_PREFIX, fleet_id, DEDUP_NAMESPACE }) catch continue;
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
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var deleted = try h.queue.commandAllowError(&.{ "DEL", key });
    deleted.deinit(h.queue.alloc);
    var fault = try h.queue.commandAllowError(&.{ "SET", key, "fault" });
    fault.deinit(h.queue.alloc);
}

fn clearStream(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
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
        "INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at) VALUES ($1::uuid, $2::uuid, $3, '# test fleet', $4::jsonb, 'active', $5, $5)",
        .{ fleet_id, fixtures.WORKSPACE_ID, name, CONFIG_PULL, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.integration_grants (uid, grant_id, fleet_id, service, status, requested_at, requested_reason, approved_at) VALUES ($1::uuid, $1, $2::uuid, 'github', 'approved', $3, 'fanout boundary test', $3)",
        .{ grant_id, fleet_id, now },
    );
}

fn clearFanoutStreams(h: *TestHarness, count: usize) void {
    for (0..count) |index| {
        var id_buf: [36]u8 = undefined;
        const fleet_id = fanoutId(&id_buf, index, false) catch continue;
        clearStream(h, fleet_id) catch |err| std.log.warn("App ingress fanout stream cleanup ignored: {s}", .{@errorName(err)});
    }
}

test "integration: App ingress routes installation repository event grant and replay" {
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
    try testing.expect(unsupported.bodyContains("\"status\":\"ignored\""));
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
