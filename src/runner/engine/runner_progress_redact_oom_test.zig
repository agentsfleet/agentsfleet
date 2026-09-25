//! Fail-closed observer guarantee (M100 §1): when arg/chunk redaction hits OOM,
//! the streaming observer DROPS the frame — it never emits the un-redacted bytes
//! that could carry a secret. The pure-function correctness of `redactBytes`
//! lives in `runner_progress_redact_test.zig`; the final-reply OOM path lives in
//! `runner_helpers_test.zig`. THIS suite pins the two *streaming* drop branches
//! (`observerRecordEvent` tool_call args, `streamCallbackThunk` chunk) that a
//! regression to `catch raw` would silently turn back into a secret leak.
//!
//! Mechanism: the Adapter's allocator is a `FailingAllocator` (redaction OOMs on
//! its first `dupe`), while the ProgressWriter keeps a working allocator — so the
//! completed frame still flows and we can prove the *args* frame was the one
//! dropped, not the whole event.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;
const providers = nullclaw.providers;
const contract = @import("contract");

const pipe_proto = @import("../pipe_proto.zig");
const runner_progress = @import("runner_progress.zig");

const SECRET = "sk-live-SUPERSECRET-007";
const PLACEHOLDER = "${secrets.llm.api_key}";

// Drain every frame the writer produced; for each, run `each(ftype, payload)`.
fn drainFrames(
    alloc: std.mem.Allocator,
    read_fd: std.posix.fd_t,
    ctx: anytype,
    comptime each: fn (@TypeOf(ctx), pipe_proto.FrameType, []const u8) void,
) !void {
    const dl = clock.nowMillis() + 5_000;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, read_fd, dl, 1 << 20)) {
            .eof, .timed_out => break,
            .frame => |f| {
                defer alloc.free(f.payload);
                each(ctx, f.ftype, f.payload);
            },
        }
    }
}

test "tool_call args frame is dropped (not emitted raw) when redaction OOMs, secret never reaches the pipe (M100 §1)" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);

    // Writer: working allocator (the completed frame must still serialize+write).
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    // Adapter: failing allocator → `redactBytes` OOMs on its first dupe.
    var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = 0 });
    const secrets = [_]runner_progress.Secret{.{ .value = SECRET, .placeholder = PLACEHOLDER }};
    var adapter = runner_progress.Adapter{
        .writer = &writer,
        .alloc = fa.allocator(),
        .secrets = &secrets,
    };

    // A completed tool call whose args carry the secret value.
    const args = "{\"path\":\"/tmp\",\"token\":\"" ++ SECRET ++ "\"}";
    const ev = observability.ObserverEvent{ .tool_call = .{
        .tool = "fs_write",
        .duration_ms = 7,
        .success = true,
        .args = args,
    } };
    const obs = adapter.observer();
    obs.vtable.record_event(obs.ptr, &ev);
    pipe_proto.testOsClose(fds[1]); // small frames fit the pipe buffer; no producer block

    const Seen = struct {
        started: bool = false,
        completed: bool = false,
        secret_on_wire: bool = false,
        fn each(self: *@This(), ftype: pipe_proto.FrameType, payload: []const u8) void {
            if (ftype != .activity) return;
            if (std.mem.indexOf(u8, payload, "tool_call_started") != null) self.started = true;
            if (std.mem.indexOf(u8, payload, "tool_call_completed") != null) self.completed = true;
            if (std.mem.indexOf(u8, payload, SECRET) != null) self.secret_on_wire = true;
        }
    };
    var seen = Seen{};
    try drainFrames(alloc, fds[0], &seen, Seen.each);

    // The args-bearing `tool_call_started` frame was DROPPED on OOM ...
    try std.testing.expect(!seen.started);
    // ... but the event was not aborted: the completed frame still closed the call ...
    try std.testing.expect(seen.completed);
    // ... and the secret value never made it onto the wire.
    try std.testing.expect(!seen.secret_on_wire);
    // The failing allocator was actually exercised (the OOM path ran, not a no-op).
    try std.testing.expect(fa.allocations == 0);
}

test "stream chunk is dropped (not emitted raw) when redaction OOMs, secret never reaches the pipe (M100 §1)" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);

    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = 0 });
    const secrets = [_]runner_progress.Secret{.{ .value = SECRET, .placeholder = PLACEHOLDER }};
    var adapter = runner_progress.Adapter{
        .writer = &writer,
        .alloc = fa.allocator(),
        .secrets = &secrets,
    };

    const chunk = providers.StreamChunk.answerDelta("partial answer leaking " ++ SECRET ++ " mid-token");
    const sc = adapter.streamCallback();
    sc.cb(sc.ctx, chunk);
    pipe_proto.testOsClose(fds[1]);

    const Seen = struct {
        any_frame: bool = false,
        secret_on_wire: bool = false,
        fn each(self: *@This(), ftype: pipe_proto.FrameType, payload: []const u8) void {
            _ = ftype;
            self.any_frame = true;
            if (std.mem.indexOf(u8, payload, SECRET) != null) self.secret_on_wire = true;
        }
    };
    var seen = Seen{};
    try drainFrames(alloc, fds[0], &seen, Seen.each);

    // The chunk frame was dropped entirely; nothing — least of all the secret —
    // was written.
    try std.testing.expect(!seen.any_frame);
    try std.testing.expect(!seen.secret_on_wire);
    try std.testing.expect(fa.allocations == 0);
    try std.testing.expect(!adapter.stream_contiguous);
    try std.testing.expectEqual(@as(u64, 1), adapter.next_stream_seq);
}

test "untyped provider bytes never enter the live pipe" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = alloc, .secrets = &.{} };
    const stream = adapter.streamCallback();
    stream.cb(stream.ctx, providers.StreamChunk.textDelta("<tool_call>PRIVATE"));
    pipe_proto.testOsClose(fds[1]);
    const read = try pipe_proto.readFrame(alloc, fds[0], clock.nowMillis() + 5_000, 1 << 20);
    try std.testing.expect(read == .eof);
    try std.testing.expectEqual(@as(u64, 0), adapter.next_stream_seq);
}

test "a secret split across two stream chunks never reaches the pipe through the live adapter (M100 §1)" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);

    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    const secrets = [_]runner_progress.Secret{.{ .value = SECRET, .placeholder = PLACEHOLDER }};
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = alloc, .secrets = &secrets };
    defer adapter.deinit(alloc); // releases the cross-chunk carry

    const sc = adapter.streamCallback();
    // Split the secret across the seam: "sk-live-SUPER" + "SECRET-007 done".
    const cut = 13; // mid-secret
    sc.cb(sc.ctx, providers.StreamChunk.answerDelta("answer " ++ SECRET[0..cut]));
    sc.cb(sc.ctx, providers.StreamChunk.answerDelta(SECRET[cut..] ++ " done"));
    pipe_proto.testOsClose(fds[1]);

    const Seen = struct {
        text: std.ArrayListUnmanaged(u8) = .empty,
        fn each(self: *@This(), a: std.mem.Allocator, ftype: pipe_proto.FrameType, payload: []const u8) void {
            if (ftype != .activity) return;
            self.text.appendSlice(a, payload) catch {};
        }
    };
    var seen = Seen{};
    defer seen.text.deinit(alloc);
    const dl = clock.nowMillis() + 5_000;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, fds[0], dl, 1 << 20)) {
            .eof, .timed_out => break,
            .frame => |f| {
                defer alloc.free(f.payload);
                seen.each(alloc, f.ftype, f.payload);
            },
        }
    }
    // The full secret was split, joined across the carry, and redacted — it
    // never appears on the wire; the placeholder does.
    try std.testing.expect(std.mem.indexOf(u8, seen.text.items, SECRET) == null);
    try std.testing.expect(std.mem.indexOf(u8, seen.text.items, PLACEHOLDER) != null);
}

test "first safe stream chunk alone carries the monotonic runtime duration" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var adapter = runner_progress.Adapter{
        .writer = &writer,
        .alloc = alloc,
        .secrets = &.{},
        .agent_runtime_started_ms = clock.nowMonotonicMillis() - 10,
    };
    defer adapter.deinit(alloc);

    const stream = adapter.streamCallback();
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("first"));
    stream.cb(stream.ctx, providers.StreamChunk.reasoningDelta("second"));
    pipe_proto.testOsClose(fds[1]);

    var seen: usize = 0;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, fds[0], clock.nowMillis() + 5_000, 1 << 20)) {
            .eof => break,
            .timed_out => return error.MissingStreamFrame,
            .frame => |frame| {
                defer alloc.free(frame.payload);
                try std.testing.expectEqual(pipe_proto.FrameType.activity, frame.ftype);
                const parsed = try std.json.parseFromSlice(contract.activity.ActivityFrame, alloc, frame.payload, .{});
                defer parsed.deinit();
                try std.testing.expect(parsed.value == .fleet_response_chunk);
                try std.testing.expectEqual(if (seen == 0) contract.activity.ActivityFrame.TextKind.answer else .reasoning, parsed.value.fleet_response_chunk.text_kind);
                if (seen == 0) {
                    try std.testing.expect(parsed.value.fleet_response_chunk.first_chunk_after_ms.? >= 10);
                    try std.testing.expect(parsed.value.fleet_response_chunk.stream_start);
                } else {
                    try std.testing.expect(parsed.value.fleet_response_chunk.first_chunk_after_ms == null);
                    try std.testing.expect(!parsed.value.fleet_response_chunk.stream_start);
                }
                try std.testing.expect(parsed.value.fleet_response_chunk.stream_contiguous);
                try std.testing.expectEqual(@as(u64, @intCast(seen)), parsed.value.fleet_response_chunk.stream_seq);
                seen += 1;
            },
        }
    }
    try std.testing.expectEqual(@as(usize, 2), seen);
}

test "failed first activity write retains timing but never grants a stream start" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var empty: [0]u8 = .{};
    var exhausted = std.heap.FixedBufferAllocator.init(&empty);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = exhausted.allocator() };
    var adapter = runner_progress.Adapter{
        .writer = &writer,
        .alloc = alloc,
        .secrets = &.{},
        .agent_runtime_started_ms = clock.nowMonotonicMillis() - 10,
    };
    defer adapter.deinit(alloc);

    const stream = adapter.streamCallback();
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("first"));
    try std.testing.expect(!adapter.first_chunk_sent);
    try std.testing.expect(!adapter.stream_contiguous);
    writer.alloc = alloc;
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("later answer"));
    pipe_proto.testOsClose(fds[1]);

    switch (try pipe_proto.readFrame(alloc, fds[0], clock.nowMillis() + 5_000, 1 << 20)) {
        .eof, .timed_out => return error.MissingStreamFrame,
        .frame => |frame| {
            defer alloc.free(frame.payload);
            const parsed = try std.json.parseFromSlice(contract.activity.ActivityFrame, alloc, frame.payload, .{});
            defer parsed.deinit();
            try std.testing.expect(parsed.value == .fleet_response_chunk);
            try std.testing.expectEqualStrings("later answer", parsed.value.fleet_response_chunk.text);
            try std.testing.expect(parsed.value.fleet_response_chunk.first_chunk_after_ms.? >= 10);
            try std.testing.expect(!parsed.value.fleet_response_chunk.stream_start);
            try std.testing.expect(!parsed.value.fleet_response_chunk.stream_contiguous);
            try std.testing.expectEqual(@as(u64, 1), parsed.value.fleet_response_chunk.stream_seq);
        },
    }
    try std.testing.expect(adapter.first_chunk_sent);
}

test "a dropped middle chunk marks every later stream frame incomplete" {
    const alloc = std.testing.allocator;
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var empty: [0]u8 = .{};
    var exhausted = std.heap.FixedBufferAllocator.init(&empty);
    var writer = runner_progress.ProgressWriter{ .fd = fds[1], .alloc = alloc };
    var adapter = runner_progress.Adapter{ .writer = &writer, .alloc = alloc, .secrets = &.{} };
    defer adapter.deinit(alloc);

    const stream = adapter.streamCallback();
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("safe"));
    writer.alloc = exhausted.allocator();
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("dropped"));
    writer.alloc = alloc;
    stream.cb(stream.ctx, providers.StreamChunk.answerDelta("later"));
    pipe_proto.testOsClose(fds[1]);

    var seen: usize = 0;
    while (true) {
        switch (try pipe_proto.readFrame(alloc, fds[0], clock.nowMillis() + 5_000, 1 << 20)) {
            .eof => break,
            .timed_out => return error.MissingStreamFrame,
            .frame => |frame| {
                defer alloc.free(frame.payload);
                const parsed = try std.json.parseFromSlice(contract.activity.ActivityFrame, alloc, frame.payload, .{});
                defer parsed.deinit();
                try std.testing.expect(parsed.value == .fleet_response_chunk);
                try std.testing.expectEqual(seen == 0, parsed.value.fleet_response_chunk.stream_contiguous);
                try std.testing.expectEqual(seen == 0, parsed.value.fleet_response_chunk.stream_start);
                try std.testing.expectEqual(if (seen == 0) @as(u64, 0) else 2, parsed.value.fleet_response_chunk.stream_seq);
                seen += 1;
            },
        }
    }
    try std.testing.expectEqual(@as(usize, 2), seen);
}
