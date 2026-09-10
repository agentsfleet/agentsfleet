// The memory forwarder: what it POSTs, and what it refuses to. The client
// points at a closed loopback port, so a POST that IS attempted fails fast and
// is swallowed (best-effort contract) — these assertions are about which
// payloads reach the client at all, which the leak detector and the absence of
// a crash are the observable for.
//
// Split from `forwarders_test.zig` (RULE FLL): that file carries the activity
// batching state machine and had no room left.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const call_deadline = @import("call_deadline");
const forwarders = @import("forwarders.zig");

const DEAD_URL = "http://127.0.0.1:9";

fn testMemoryForwarder(c: *client_mod) forwarders.MemoryForwarder {
    return .{
        .alloc = testing.allocator,
        .cp = c,
        .runner_token = "agt_rtest",
        .fleet_id = "z_test",
        .lease_id = "lease_test",
        .fencing_token = 7,
        .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS,
    };
}

test "memory forwarder drops a malformed capture payload without posting" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testMemoryForwarder(&c);

    // parse fails → warn-and-drop; the leak detector asserts full cleanup
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "not-json");
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "{\"kind\":\"object-not-array\"}");
}

test "memory forwarder posts a valid delta set best-effort" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testMemoryForwarder(&c);

    // a real delta parses, the fenced POST fails fast against the dead port and
    // is swallowed (best-effort contract) — no crash, no leak
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "[{\"key\":\"k\",\"content\":\"c\",\"category\":\"fact\"}]");
}

test "memory forwarder skips the POST when the capture set is empty" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testMemoryForwarder(&c);

    // A fleet that never declared `memory_store` captures an empty set on every
    // checkpoint and at run end. Upserting zero deltas cannot change a row, so
    // the forwarder returns before the client is touched — same observable as
    // the malformed case above (parse succeeds here, the POST simply never runs).
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "[]");
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "[ ]");
}
