// Live-Redis tests for the async gate primitives. Skip when REDIS_URL is
// absent (same gating as the queue integration suites).

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const ag_async = @import("approval_gate_async.zig");
const approval_gate = @import("approval_gate.zig");
const ec = @import("../errors/error_registry.zig");
const queue_redis = @import("../queue/redis_client.zig");

const REDIS_URL_ENV = "REDIS_URL";

fn liveClient() !queue_redis.Client {
    // testLiveValue returns a borrowed slice into the live environment — never free it.
    const url = common.env.testLiveValue(REDIS_URL_ENV) orelse return error.SkipZigTest;
    return queue_redis.Client.connectFromUrl(common.globalIo(), std.testing.allocator, url) catch return error.SkipZigTest;
}

fn delKey(client: *queue_redis.Client, key: []const u8) void {
    var resp = client.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(client.alloc);
}

fn cleanupGateKeys(client: *queue_redis.Client, agent_id: []const u8, event_id: []const u8, action_id: []const u8) void {
    var buf_a: [256]u8 = undefined;
    if (std.fmt.bufPrint(&buf_a, "{s}{s}:{s}", .{ ec.GATE_EVENT_REF_KEY_PREFIX, agent_id, event_id })) |k| delKey(client, k) else |_| {}
    var buf_b: [256]u8 = undefined;
    if (std.fmt.bufPrint(&buf_b, "{s}{s}", .{ ec.GATE_RESPONSE_KEY_PREFIX, action_id })) |k| delKey(client, k) else |_| {}
}

test "gate ref: pending until approved — the next poll proceeds" {
    var client = try liveClient();
    defer client.deinit();

    const agent_id = "z-gate-async-approve";
    const event_id = "ev-gate-async-1";
    const action_id = "0193e9a0-0000-7000-8000-00000000aaaa";
    defer cleanupGateKeys(&client, agent_id, event_id, action_id);

    const deadline = clock.nowMillis() + 60_000;
    try ag_async.recordEventGateRef(&client, agent_id, event_id, action_id, deadline);

    const ref = (try ag_async.lookupEventGateRef(&client, agent_id, event_id)).?;
    try std.testing.expectEqualStrings(action_id, ref.actionId());
    try std.testing.expectEqual(deadline, ref.deadline_ms);

    // No decision yet, deadline ahead → pending (lease answers no-work).
    try std.testing.expectEqual(ag_async.PendingEval.pending, try ag_async.evaluateRef(&client, &ref, clock.nowMillis()));

    // Human approves → the next poll proceeds.
    try approval_gate.resolveApproval(&client, action_id, ec.GATE_DECISION_APPROVE);
    try std.testing.expectEqual(ag_async.PendingEval.approved, try ag_async.evaluateRef(&client, &ref, clock.nowMillis()));
}

test "gate ref: deny decision blocks" {
    var client = try liveClient();
    defer client.deinit();

    const agent_id = "z-gate-async-deny";
    const event_id = "ev-gate-async-2";
    const action_id = "0193e9a0-0000-7000-8000-00000000bbbb";
    defer cleanupGateKeys(&client, agent_id, event_id, action_id);

    try ag_async.recordEventGateRef(&client, agent_id, event_id, action_id, clock.nowMillis() + 60_000);
    try approval_gate.resolveApproval(&client, action_id, ec.GATE_DECISION_DENY);

    const ref = (try ag_async.lookupEventGateRef(&client, agent_id, event_id)).?;
    try std.testing.expectEqual(ag_async.PendingEval.denied, try ag_async.evaluateRef(&client, &ref, clock.nowMillis()));
}

test "gate ref: deadline passed without decision evaluates expired" {
    var client = try liveClient();
    defer client.deinit();

    const agent_id = "z-gate-async-expire";
    const event_id = "ev-gate-async-3";
    const action_id = "0193e9a0-0000-7000-8000-00000000cccc";
    defer cleanupGateKeys(&client, agent_id, event_id, action_id);

    const deadline = clock.nowMillis() - 1;
    try ag_async.recordEventGateRef(&client, agent_id, event_id, action_id, deadline);

    const ref = (try ag_async.lookupEventGateRef(&client, agent_id, event_id)).?;
    try std.testing.expectEqual(ag_async.PendingEval.expired, try ag_async.evaluateRef(&client, &ref, clock.nowMillis()));
}

test "gate ref: absent ref reads as null (first encounter)" {
    var client = try liveClient();
    defer client.deinit();
    try std.testing.expect((try ag_async.lookupEventGateRef(&client, "z-gate-async-none", "ev-never-recorded")) == null);
}
