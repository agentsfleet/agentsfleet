const std = @import("std");
const constants = @import("common");
const protocol = @import("contract").protocol;
const runner_row = @import("runner_row.zig");

test "deriveLiveness: never-seen sentinel is registered regardless of lease" {
    const now: i64 = 1_000_000;
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, runner_row.deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, runner_row.deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, true, now));
}

test "deriveLiveness: a live lease is busy even when last_seen is stale" {
    const now: i64 = 10_000_000;
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1;
    try std.testing.expectEqual(protocol.RunnerLiveness.busy, runner_row.deriveLiveness(stale, true, now));
}

test "deriveLiveness: fresh heartbeat without a lease is online; stale is offline" {
    const now: i64 = 10_000_000;
    const fresh = now - 1;
    const at_threshold = now - constants.RUNNER_OFFLINE_AFTER_MS;
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1;
    try std.testing.expectEqual(protocol.RunnerLiveness.online, runner_row.deriveLiveness(fresh, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.online, runner_row.deriveLiveness(at_threshold, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.offline, runner_row.deriveLiveness(stale, false, now));
}
