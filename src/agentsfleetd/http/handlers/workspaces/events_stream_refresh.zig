//! Maps a fan-in synchronization tick to the stream-loop decision.

const logging = @import("log");
const FanIn = @import("events_stream_fanin.zig");

const log = logging.scoped(.http_workspace_events_stream);

pub const Outcome = enum { unchanged, changed, deferred, revoked };

pub fn run(fanin: *FanIn, now_ms: i64) Outcome {
    switch (fanin.sync(now_ms)) {
        .unchanged => return .unchanged,
        .deferred => return .deferred,
        .changed => |delta| {
            log.debug("workspace_stream_fleet_set_changed", .{
                .workspace_id = fanin.workspace_id,
                .added = delta.added,
                .removed = delta.removed,
                .fanned_in = fanin.channelCount(),
            });
            return .changed;
        },
        .revoked => {
            log.info("workspace_stream_revoked", .{ .workspace_id = fanin.workspace_id });
            return .revoked;
        },
    }
}
