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

    const chunk = providers.StreamChunk.textDelta("partial answer leaking " ++ SECRET ++ " mid-token");
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
    sc.cb(sc.ctx, providers.StreamChunk.textDelta("answer " ++ SECRET[0..cut]));
    sc.cb(sc.ctx, providers.StreamChunk.textDelta(SECRET[cut..] ++ " done"));
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
