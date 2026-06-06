//! Mid-run memory-checkpoint cadence: a checkpoint-due tool-call tick flushes
//! the in-run store to the parent as a `.memory` frame, so a long run's learned
//! memory is durable before it finishes (not only at run end).

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const protocol = @import("contract").protocol;

const pipe_proto = @import("../pipe_proto.zig");
const inrun_memory = @import("inrun_memory.zig");
const runner_progress = @import("runner_progress.zig");

test "a memory checkpoint-due tick flushes the in-run store as a .memory frame" {
    const alloc = std.testing.allocator;
    var rt = inrun_memory.initRuntime(alloc, "/tmp") orelse return error.SkipZigTest;
    defer rt.deinit();
    try rt.memory.store("learned", "a durable fact", .core, null);

    const fds = try pipe_proto.osPipe();
    defer pipe_proto.osClose(fds[0]);

    var capturer = inrun_memory.MemoryCapturer{ .mem = rt.memory, .fd = fds[1], .alloc = alloc };
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var adapter = runner_progress.Adapter{
        .writer = &writer,
        .alloc = alloc,
        .secrets = &[_]runner_progress.Secret{},
        .memory_checkpoint_every = 1, // every tool call is a checkpoint
        .memory_capturer = &capturer,
    };

    // One completed tool call → checkpoint-due (1 % 1 == 0) → a capture frame.
    const ev = observability.ObserverEvent{ .tool_call = .{ .tool = "fs_read", .duration_ms = 1, .success = true } };
    const obs = adapter.observer();
    obs.vtable.record_event(obs.ptr, &ev);
    pipe_proto.osClose(fds[1]); // small frames fit the pipe buffer; no producer block

    // Drain every frame; assert a `.memory` frame carrying the entry appeared.
    var saw_memory = false;
    const dl = clock.nowMillis() + 5_000;
    drain: while (true) {
        switch (try pipe_proto.readFrame(alloc, fds[0], dl, 1 << 20)) {
            .eof, .timed_out => break :drain,
            .frame => |f| {
                defer alloc.free(f.payload);
                if (f.ftype != .memory) continue :drain;
                saw_memory = true;
                const parsed = try std.json.parseFromSlice([]protocol.MemoryDelta, alloc, f.payload, .{});
                defer parsed.deinit();
                try std.testing.expect(parsed.value.len >= 1);
                try std.testing.expectEqualStrings("learned", parsed.value[0].key);
            },
        }
    }
    try std.testing.expect(saw_memory);
}
