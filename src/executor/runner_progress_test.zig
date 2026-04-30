//! Tests for `runner_progress.Adapter`'s context-lifecycle counters
//! (M41 §4 memory_checkpoint_every and §5 tool_window). Split from the
//! source file to keep both files under the 350-line cap; the
//! redactBytes tests stay inline in `runner_progress.zig` because they
//! exercise a file-private helper.

const std = @import("std");
const observability = @import("nullclaw").observability;

const runner_progress = @import("runner_progress.zig");
const progress_writer_mod = @import("progress_writer.zig");

const Adapter = runner_progress.Adapter;

/// Drive the observer with a synthetic completed-tool-call event,
/// going through the public Observer vtable so we don't reach into
/// file-private helpers in `runner_progress.zig`. NullClaw passes the
/// event by const pointer; the slice fields are borrowed for the
/// duration of the call.
fn fireToolCall(adapter: *Adapter, tool_name: []const u8) void {
    const evt = observability.ObserverEvent{
        .tool_call = .{
            .tool = tool_name,
            .duration_ms = 0,
            .success = true,
            .args = null,
        },
    };
    const obs = adapter.observer();
    obs.vtable.record_event(obs.ptr, &evt);
}

fn newAdapterWithStubWriter(
    w: *progress_writer_mod,
    memory_checkpoint_every: u32,
    tool_window: u32,
) Adapter {
    return .{
        .writer = w,
        .alloc = std.testing.allocator,
        .secrets = &.{},
        .memory_checkpoint_every = memory_checkpoint_every,
        .tool_window = tool_window,
    };
}

fn newStubWriter() progress_writer_mod {
    // fd=-1: progress_writer_mod.write swallows sends to a closed/invalid
    // fd. The bookkeeping path on the Adapter still runs, which is what
    // these tests exercise.
    return .{
        .fd = -1,
        .request_id = 0,
        .alloc = std.testing.allocator,
    };
}

// ── L1 memory_checkpoint_every cadence ────────────────────────────────────

test "memory_checkpoint_every=0 disables the nudge counter" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 0, 0);
    var i: usize = 0;
    while (i < 5) : (i += 1) fireToolCall(&adapter, "shell");
    try std.testing.expectEqual(@as(u32, 5), adapter.tool_call_count);
    try std.testing.expectEqual(@as(u32, 0), adapter.nudges_emitted);
}

test "memory_checkpoint_every=5 fires nudge at every 5th completed tool call" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 5, 0);
    var i: usize = 0;
    while (i < 5) : (i += 1) fireToolCall(&adapter, "shell");
    try std.testing.expectEqual(@as(u32, 5), adapter.tool_call_count);
    try std.testing.expectEqual(@as(u32, 1), adapter.nudges_emitted);

    while (i < 12) : (i += 1) fireToolCall(&adapter, "http_request");
    try std.testing.expectEqual(@as(u32, 12), adapter.tool_call_count);
    // Fired at 5 and 10, not yet at 12.
    try std.testing.expectEqual(@as(u32, 2), adapter.nudges_emitted);
}

test "memory_checkpoint_every=1 fires on every call (extreme case)" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 1, 0);
    var i: usize = 0;
    while (i < 3) : (i += 1) fireToolCall(&adapter, "shell");
    try std.testing.expectEqual(@as(u32, 3), adapter.nudges_emitted);
}

// ── L2 tool_window cadence ────────────────────────────────────────────────

test "tool_window=0 disables window-exceeded logging" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 0, 0);
    var i: usize = 0;
    while (i < 30) : (i += 1) fireToolCall(&adapter, "shell");
    try std.testing.expectEqual(@as(u32, 30), adapter.tool_call_count);
    try std.testing.expectEqual(@as(u32, 0), adapter.window_exceeded_logs);
}

test "tool_window=20 starts logging after the 20th call" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 0, 20);
    var i: usize = 0;
    while (i < 20) : (i += 1) fireToolCall(&adapter, "http_request");
    // Call #20: count == window, NOT exceeded yet (strict greater-than).
    try std.testing.expectEqual(@as(u32, 0), adapter.window_exceeded_logs);

    while (i < 25) : (i += 1) fireToolCall(&adapter, "http_request");
    // Calls #21..#25 each emit a log line.
    try std.testing.expectEqual(@as(u32, 5), adapter.window_exceeded_logs);
    try std.testing.expectEqual(@as(u32, 25), adapter.tool_call_count);
}

test "L1 + L2 thresholds fire independently" {
    var w = newStubWriter();
    var adapter = newAdapterWithStubWriter(&w, 5, 10);
    var i: usize = 0;
    while (i < 12) : (i += 1) fireToolCall(&adapter, "shell");
    // L1 fired at 5 and 10 → 2 nudges.
    try std.testing.expectEqual(@as(u32, 2), adapter.nudges_emitted);
    // L2 fired on calls 11 and 12 → 2 logs.
    try std.testing.expectEqual(@as(u32, 2), adapter.window_exceeded_logs);
}
