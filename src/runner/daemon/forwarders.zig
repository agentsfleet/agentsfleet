//! The two child-stream forwarders the lease executor wires into the
//! supervisor: live-tail activity frames (batched per flush window — one POST
//! per batch, not per frame) and mid-run memory captures. Extracted from
//! loop.zig by concern; loop.zig remains the public lease-loop API.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const contract = @import("contract");

const client_mod = @import("control_plane_client.zig");
const protocol = contract.protocol;

const log = logging.scoped(.agent_runner);

/// Activity frames batch per POST: flush at this many frames…
pub const ACTIVITY_BATCH_MAX_FRAMES: usize = 16;
/// …or this many buffered bytes (caps retained memory for chatty frames)…
pub const ACTIVITY_BATCH_MAX_BYTES: usize = 64 * 1024;
/// …or when the oldest buffered frame is this stale (live-tail latency budget).
pub const ACTIVITY_FLUSH_WINDOW_MS: i64 = 1_000;

/// Batches the `activity` frames the sandboxed child streams and forwards them
/// to the control plane per flush window — one POST per batch, not per frame
/// (a chatty agent run no longer costs one round-trip per tool call). Frames
/// serialize on arrival (their slices are only valid during `forward`); the
/// flush fires at the frame/byte caps, when the oldest buffered frame exceeds
/// the window (driven by the supervisor tick), and finally at end of run.
/// Best-effort by contract — transport errors are swallowed, so a dropped
/// live-tail batch never disturbs execution.
pub const ActivityForwarder = struct {
    alloc: std.mem.Allocator,
    cp: *client_mod,
    runner_token: []const u8,
    lease_id: []const u8,
    deadline_ms: u31,
    // BUFFER GATE: ArrayList(u8) — append-as-you-go accumulation of serialized
    // frames, read once per flush.
    buf: std.ArrayList(u8) = .empty,
    count: usize = 0,
    first_buffered_ms: i64 = 0,

    pub fn forward(ctx: *anyopaque, frame: contract.activity.ActivityFrame) void {
        const self: *ActivityForwarder = @ptrCast(@alignCast(ctx));
        const json = std.json.Stringify.valueAlloc(self.alloc, frame, .{}) catch return;
        defer self.alloc.free(json);
        if (self.count == 0) self.first_buffered_ms = clock.nowMillis();
        const valid_len = self.buf.items.len;
        if (self.count > 0) self.buf.append(self.alloc, ',') catch return;
        self.buf.appendSlice(self.alloc, json) catch {
            // roll the orphan comma back — a half-appended frame would poison
            // the whole batch into invalid JSON, not just drop this frame
            self.buf.shrinkRetainingCapacity(valid_len);
            return;
        };
        self.count += 1;
        const stale = clock.nowMillis() - self.first_buffered_ms >= ACTIVITY_FLUSH_WINDOW_MS;
        if (self.count >= ACTIVITY_BATCH_MAX_FRAMES or self.buf.items.len >= ACTIVITY_BATCH_MAX_BYTES or stale) {
            self.flush();
        }
    }

    /// Tick-driven flush so a quiet child's tail frames still ship within the
    /// live-tail window instead of waiting for the next frame or end of run.
    pub fn flushIfStale(self: *ActivityForwarder, now_ms: i64) void {
        if (self.count > 0 and now_ms - self.first_buffered_ms >= ACTIVITY_FLUSH_WINDOW_MS) self.flush();
    }

    pub fn flush(self: *ActivityForwarder) void {
        if (self.count == 0) return;
        self.cp.activityFramesJson(self.alloc, self.runner_token, self.lease_id, self.buf.items, self.deadline_ms);
        self.buf.clearRetainingCapacity();
        self.count = 0;
    }

    pub fn deinit(self: *ActivityForwarder) void {
        self.buf.deinit(self.alloc);
    }
};

/// POSTs each `.memory` capture frame the child writes to the control plane —
/// the daemon (not the child) holds the `agt_r` token, so capture rides the
/// trusted plane. The frame is a JSON array of deltas; the daemon wraps it with
/// the held lease's `lease_id` + `fencing_token` so the write is fenced. A blip
/// is logged and swallowed — the next capture re-sends the full set.
pub const MemoryForwarder = struct {
    alloc: std.mem.Allocator,
    cp: *client_mod,
    runner_token: []const u8,
    agent_id: []const u8,
    lease_id: []const u8,
    fencing_token: u64,
    deadline_ms: u31,

    pub fn forward(ctx: *anyopaque, payload: []const u8) void {
        const self: *MemoryForwarder = @ptrCast(@alignCast(ctx));
        const parsed = std.json.parseFromSlice([]protocol.MemoryDelta, self.alloc, payload, .{}) catch {
            log.warn("memory_frame_parse_failed", .{ .agent_id = self.agent_id });
            return;
        };
        defer parsed.deinit();
        const req = protocol.MemoryPushRequest{
            .lease_id = self.lease_id,
            .fencing_token = self.fencing_token,
            .memory = parsed.value,
        };
        self.cp.memoryCapture(self.alloc, self.runner_token, self.agent_id, req, self.deadline_ms) catch |err|
            log.warn("memory_capture_post_failed", .{ .agent_id = self.agent_id, .err = @errorName(err) });
    }
};

test {
    _ = @import("forwarders_test.zig");
}
