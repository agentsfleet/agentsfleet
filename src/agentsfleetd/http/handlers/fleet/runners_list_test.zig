const std = @import("std");
const constants = @import("common");
const protocol = @import("contract").protocol;
const runners_list = @import("runners_list.zig");

test "deriveLiveness: never-seen sentinel is registered regardless of lease" {
    const now: i64 = 1_000_000;
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, runners_list.deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, runners_list.deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, true, now));
}

test "deriveLiveness: a live lease is busy even when last_seen is stale" {
    const now: i64 = 10_000_000;
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1;
    try std.testing.expectEqual(protocol.RunnerLiveness.busy, runners_list.deriveLiveness(stale, true, now));
}

test "deriveLiveness: fresh heartbeat without a lease is online; stale is offline" {
    const now: i64 = 10_000_000;
    const fresh = now - 1;
    const at_threshold = now - constants.RUNNER_OFFLINE_AFTER_MS;
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1;
    try std.testing.expectEqual(protocol.RunnerLiveness.online, runners_list.deriveLiveness(fresh, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.online, runners_list.deriveLiveness(at_threshold, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.offline, runners_list.deriveLiveness(stale, false, now));
}
