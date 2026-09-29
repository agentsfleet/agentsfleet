//! Every frame of one tool call carries that call's id, and no two calls share
//! one — even two calls of the same tool, which a reader pairing by name and
//! timing cannot tell apart.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const contract = @import("contract");

const pipe_proto = @import("../pipe_proto.zig");
const runner_progress = @import("runner_progress.zig");

const ActivityFrame = contract.activity.ActivityFrame;
const Tag = std.meta.Tag(ActivityFrame);

const TOOL = "fs_read";
const READ_BUDGET_MS = 5_000;
const MAX_FRAME_BYTES = 1 << 20;

/// One frame as a reader pairs it: its kind and the call it names.
const Seen = struct { tag: Tag, call_id: []const u8 };

test "test_runner_stamps_one_call_id_per_call" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = alloc, .secrets = &.{} };
    const obs = adapter.observer();

    const start = observability.ObserverEvent{ .tool_call_start = .{ .tool = TOOL } };
    const done = observability.ObserverEvent{ .tool_call = .{ .tool = TOOL, .duration_ms = 1, .success = true, .args = "{\"path\":\"a\"}" } };
    // Two calls of one tool, then a completion whose start was never reported.
    for ([_]*const observability.ObserverEvent{ &start, &done, &start, &done, &done }) |event| {
        obs.vtable.record_event(obs.ptr, event);
    }
    pipe_proto.testOsClose(fds[1]); // small frames fit the pipe buffer; no producer block

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const seen = try readSeen(arena.allocator(), fds[0]);
    const expected = [_]Seen{
        .{ .tag = .tool_call_started, .call_id = "1" },
        .{ .tag = .tool_call_started, .call_id = "1" },
        .{ .tag = .tool_call_completed, .call_id = "1" },
        .{ .tag = .tool_call_started, .call_id = "2" },
        .{ .tag = .tool_call_started, .call_id = "2" },
        .{ .tag = .tool_call_completed, .call_id = "2" },
        .{ .tag = .tool_call_started, .call_id = "3" },
        .{ .tag = .tool_call_completed, .call_id = "3" },
    };
    try std.testing.expectEqual(expected.len, seen.len);
    for (expected, seen) |want, got| {
        try std.testing.expectEqual(want.tag, got.tag);
        try std.testing.expectEqualStrings(want.call_id, got.call_id);
    }
}

// Every activity frame on `fd` until EOF, parsed into `arena`.
fn readSeen(arena: std.mem.Allocator, fd: std.posix.fd_t) ![]const Seen {
    var seen: std.ArrayList(Seen) = .empty;
    const deadline = clock.nowMillis() + READ_BUDGET_MS;
    while (true) {
        switch (try pipe_proto.readFrame(arena, fd, deadline, MAX_FRAME_BYTES)) {
            .eof, .timed_out => return seen.toOwnedSlice(arena),
            .frame => |f| {
                if (f.ftype != .activity) continue;
                const frame = try std.json.parseFromSliceLeaky(ActivityFrame, arena, f.payload, .{});
                const call_id = switch (frame) {
                    .tool_call_started => |b| b.call_id,
                    .tool_call_completed => |b| b.call_id,
                    .tool_call_progress => |b| b.call_id,
                    .fleet_response_chunk => null,
                };
                try seen.append(arena, .{ .tag = frame, .call_id = call_id orelse return error.CallIdMissing });
            },
        }
    }
}
