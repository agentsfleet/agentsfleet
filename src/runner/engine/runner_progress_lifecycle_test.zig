//! Context-lifecycle observability (L2 window / L3 chunk threshold) and the
//! observer vtable surface.
//!
//! The L2/L3 branches and four of the six vtable thunks had no executed lines.
//! They are the runtime's only visibility into a runaway fleet — SKILL prose
//! tells the fleet to compact, and these counters are how on-call confirms the
//! prompt landed — so each layer's trip condition is pinned, including the
//! disabled-by-zero short-circuits.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;

const pipe_proto = @import("../pipe_proto.zig");
const runner_progress = @import("runner_progress.zig");

const NO_SECRETS = [_]runner_progress.Secret{};

const Harness = struct {
    fds: [2]std.posix.fd_t,
    writer: runner_progress.ProgressWriter,

    fn init(alloc: std.mem.Allocator) !Harness {
        const fds = try pipe_proto.testOsPipe();
        return .{ .fds = fds, .writer = .{ .fd = fds[1], .alloc = alloc } };
    }

    fn deinit(self: *Harness) void {
        pipe_proto.testOsClose(self.fds[0]);
        if (self.fds[1] >= 0) pipe_proto.testOsClose(self.fds[1]);
    }

    /// Close the write end early (EOF for a reader) — deinit then skips it.
    fn closeWrite(self: *Harness) void {
        pipe_proto.testOsClose(self.fds[1]);
        self.fds[1] = -1;
    }
};

fn toolCall(name: []const u8) observability.ObserverEvent {
    return .{ .tool_call = .{ .tool = name, .duration_ms = 1, .success = true } };
}

test "L2: calls past the window each log, calls inside it stay quiet" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{
        .writer = &h.writer,
        .alloc = alloc,
        .secrets = &NO_SECRETS,
        .tool_window = 2,
    };
    const obs = adapter.observer();

    const ev = toolCall("fs_read");
    obs.vtable.record_event(obs.ptr, &ev); // 1 — inside the window
    obs.vtable.record_event(obs.ptr, &ev); // 2 — at the window, still quiet
    try std.testing.expectEqual(@as(u32, 0), adapter.window_exceeded_logs);

    obs.vtable.record_event(obs.ptr, &ev); // 3 — exceeded
    obs.vtable.record_event(obs.ptr, &ev); // 4 — every subsequent call logs
    try std.testing.expectEqual(@as(u32, 2), adapter.window_exceeded_logs);
    try std.testing.expectEqual(@as(u32, 4), adapter.tool_call_count);
}

test "L2: a zero window disables the layer no matter the call count" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{
        .writer = &h.writer,
        .alloc = alloc,
        .secrets = &NO_SECRETS,
        .tool_window = 0,
    };
    const obs = adapter.observer();
    const ev = toolCall("fs_read");
    for (0..5) |_| obs.vtable.record_event(obs.ptr, &ev);
    try std.testing.expectEqual(@as(u32, 0), adapter.window_exceeded_logs);
}

fn llmResponse(prompt_tokens: ?u32) observability.ObserverEvent {
    return .{ .llm_response = .{
        .provider = "openai",
        .model = "probe",
        .duration_ms = 1,
        .success = true,
        .error_message = null,
        .prompt_tokens = prompt_tokens,
    } };
}

test "L3: crossing the fill threshold logs and records the prompt size" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{
        .writer = &h.writer,
        .alloc = alloc,
        .secrets = &NO_SECRETS,
        .stage_chunk_threshold = 0.5,
        .context_cap_tokens = 100,
    };
    const obs = adapter.observer();

    const below = llmResponse(40); // 40% — under threshold
    obs.vtable.record_event(obs.ptr, &below);
    try std.testing.expectEqual(@as(u32, 0), adapter.chunk_threshold_logs);
    try std.testing.expectEqual(@as(u32, 40), adapter.last_prompt_tokens);

    const above = llmResponse(80); // 80% — breached
    obs.vtable.record_event(obs.ptr, &above);
    try std.testing.expectEqual(@as(u32, 1), adapter.chunk_threshold_logs);
    try std.testing.expectEqual(@as(u32, 80), adapter.last_prompt_tokens);
}

test "L3: a zero cap or absent prompt count short-circuits, never divides" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{
        .writer = &h.writer,
        .alloc = alloc,
        .secrets = &NO_SECRETS,
        .stage_chunk_threshold = 0.5,
        .context_cap_tokens = 0, // no denominator → the layer must stay off
    };
    const obs = adapter.observer();
    const ev = llmResponse(1_000_000);
    obs.vtable.record_event(obs.ptr, &ev);
    try std.testing.expectEqual(@as(u32, 0), adapter.chunk_threshold_logs);

    adapter.context_cap_tokens = 100;
    const no_tokens = llmResponse(null); // provider reported nothing
    obs.vtable.record_event(obs.ptr, &no_tokens);
    try std.testing.expectEqual(@as(u32, 0), adapter.chunk_threshold_logs);
}

test "a tool_call_start frames the tool immediately with empty redacted args" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{ .writer = &h.writer, .alloc = alloc, .secrets = &NO_SECRETS };
    const obs = adapter.observer();

    const ev = observability.ObserverEvent{ .tool_call_start = .{ .tool = "bash" } };
    obs.vtable.record_event(obs.ptr, &ev);
    h.closeWrite();

    const dl = clock.nowMillis() + 5_000;
    var saw = false;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, h.fds[0], dl, 1 << 20)) {
            .eof, .timed_out => break,
            .frame => |f| {
                defer alloc.free(f.payload);
                if (std.mem.indexOf(u8, f.payload, "tool_call_started") != null and
                    std.mem.indexOf(u8, f.payload, "\"bash\"") != null) saw = true;
            },
        }
    }
    try std.testing.expect(saw);
}

test "the observer vtable's passive surface answers without a fleet wired" {
    const alloc = std.testing.allocator;
    var h = try Harness.init(alloc);
    defer h.deinit();
    var adapter = runner_progress.Adapter{ .writer = &h.writer, .alloc = alloc, .secrets = &NO_SECRETS };
    const obs = adapter.observer();

    // tokens_used with no fleet: emitUsage is a documented no-op, not a crash.
    const metric = observability.ObserverMetric{ .tokens_used = 10 };
    obs.vtable.record_metric(obs.ptr, &metric);

    obs.vtable.flush(obs.ptr);
    try std.testing.expectEqualStrings("agentsfleet-runner-progress", obs.vtable.name(obs.ptr));
    try std.testing.expect(obs.vtable.get_trace_id(obs.ptr) == null);
    obs.vtable.set_trace_id(obs.ptr, [_]u8{0} ** 32);
}
