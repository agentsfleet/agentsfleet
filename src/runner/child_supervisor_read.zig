//! child_supervisor_read.zig — the framed-stdout read loop (parent side).
//!
//! Split out of `child_supervisor.zig` to keep both files within the line
//! budget: this module owns the child→parent message plane — the activity /
//! memory / usage sinks, the renewal hook the daemon installs, and the loop
//! that reads framed stdout up to the terminal `result` frame while driving
//! renewal ticks. `child_supervisor.zig` re-exports the public names so callers
//! and tests keep using `child_supervisor.{ActivitySink,RenewHook,readResult,…}`.
//!
//! One thread runs this loop: frame parsing and the renewal `onTick` never race,
//! so the folded usage snapshot is a plain field (no atomics).

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");
const cred = @import("engine/credential_request.zig");
const result_mod = @import("child_supervisor_result.zig");
const types = @import("engine/types.zig");
const client_errors = @import("engine/client_errors.zig");

const log = logging.scoped(.runner_supervisor);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

const ActivityFrame = contract.activity.ActivityFrame;
pub const ReadOutcome = result_mod.ReadOutcome;

/// Cap on the serialized result we read back from a child (defensive against a
/// runaway child flooding stdout).
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;

/// Best-effort sink for the `activity` frames the child streams while running.
/// The parent forwards each to the control plane (`POST .../activity`); a
/// dropped frame is cosmetic (the durable record is `report`), so `forward`
/// returns void and never fails the lease.
pub const ActivitySink = struct {
    ctx: *anyopaque,
    forward: *const fn (ctx: *anyopaque, frame: ActivityFrame) void,
};

/// Best-effort sink for the child's `.memory` capture frames. `payload` is the
/// raw frame bytes (a JSON array of `MemoryDelta`); the daemon parses + POSTs
/// them. A dropped frame is recoverable (the next capture re-sends the full
/// set), so `forward` returns void and never fails the lease.
pub const MemorySink = struct {
    ctx: *anyopaque,
    forward: *const fn (ctx: *anyopaque, payload: []const u8) void,
};

/// What the read loop should do after a renewal tick or a progress frame.
/// `extend` carries the new absolute kill deadline (epoch ms).
/// `terminate` carries the class the run is reported under, so a fleet-budget
/// stop reaches the durable `failure_label` instead of collapsing into the
/// generic `renewal_terminate` every renewal stop used to share.
pub const RenewDecision = union(enum) { keep, extend: i64, terminate: types.FailureClass };

/// Outcome of servicing one `credential_request` (M102 §3): a short-lived token
/// for the child, or a typed rejection it fails closed on. `token` is owned by the
/// `alloc` handed to `onMint`; the read loop frees it after framing the reply.
pub const CredentialOutcome = union(enum) {
    minted: struct { token: []const u8, expires_at_ms: i64 },
    rejected,
};

/// Hook the daemon installs so the supervisor can mint on the child's behalf
/// without the read loop knowing any HTTP. `onMint` forwards the ask to the
/// daemon broker over the agt_r plane (`control_plane_client.mint`), binding the
/// mint to the lease's workspace server-side (Invariant 2). It never logs the
/// token (VLT). A null hook means mint is unconfigured — every ask is rejected.
pub const MintHook = struct {
    ctx: *anyopaque,
    onMint: *const fn (ctx: *anyopaque, alloc: std.mem.Allocator, integration: []const u8, scope: ?[]const u8) CredentialOutcome,
};

/// Hook the daemon installs so the supervisor can drive lease renewal during a
/// long execution without the supervisor knowing any HTTP. `onTick` fires in
/// the idle gap between frames (renewal-tick cadence) and after each progress
/// frame, carrying the current epoch ms and the latest cumulative usage
/// snapshot (zeros until the child's first usage frame); the daemon renews
/// inside the window and returns a decision. A live child that emits no
/// frames still ticks, so a long run renews and is never falsely reclaimed.
pub const RenewHook = struct {
    ctx: *anyopaque,
    onTick: *const fn (ctx: *anyopaque, now_ms: i64, usage: pipe_proto.UsageSnapshot) RenewDecision,
    /// How often (ms) the read loop wakes between frames to consider renewal.
    /// Production sets `constants.RENEWAL_TICK_MS`; tests inject a small value.
    tick_ms: i64,
};

/// Read the child's framed stdout up to the terminal `result` frame, bounded by
/// the lease deadline. Each `activity` frame is forwarded best-effort and freed;
/// the `result` frame's bytes are returned (caller-owned). EOF before a result
/// yields empty bytes (the caller classifies that as a transport loss); deadline
/// elapse sets `timed_out` and the caller kills the child.
pub fn readResult(
    alloc: std.mem.Allocator,
    fd: std.posix.fd_t,
    /// Child's stdin (parent→child): where a `credential_response` is framed back
    /// when the child raises a `credential_request`. The lease's response channel.
    response_fd: std.posix.fd_t,
    deadline_ms: i64,
    sink: ActivitySink,
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
    /// Services on-demand mint asks (M102 §3); null ⇒ every ask is rejected.
    mint_hook: ?MintHook,
) !ReadOutcome {
    var deadline = deadline_ms;
    // Frame parsing and renewal ticks share this one read-loop thread (every
    // onTick runs between reads) — plain fields, no atomics; @max-fold = no regress.
    var usage = pipe_proto.UsageSnapshot{};
    while (true) {
        const tick_deadline = if (renew_hook) |h|
            @min(deadline, clock.nowMillis() + h.tick_ms)
        else
            deadline;
        switch (try pipe_proto.waitReadable(fd, tick_deadline)) {
            .timed_out => {
                const now = clock.nowMillis();
                if (now >= deadline) return .{ .timed_out = true };
                if (applyTick(renew_hook, &deadline, now, usage)) |reason| return .{ .terminated = true, .terminate_reason = reason };
                continue;
            },
            .readable => {},
        }
        // Data is present: read one whole frame at the full lease deadline so a
        // tick never interrupts a frame mid-read (which would desync the stream).
        switch (try pipe_proto.readFrame(alloc, fd, deadline, MAX_RESULT_BYTES)) {
            .timed_out => return .{ .timed_out = true },
            .eof => return .{},
            .frame => |f| if (handleFrame(alloc, f, response_fd, sink, mem_sink, renew_hook, mint_hook, &deadline, &usage)) |outcome|
                return outcome,
        }
    }
}

/// Dispatch one decoded non-control frame: forward it to its sink (or fold a
/// usage snapshot), then run the renewal tick. Returns a terminal `ReadOutcome`
/// to propagate — a `result` frame's bytes (ownership transfers to the caller),
/// or a hook `.terminate` — else null to keep reading. The `activity`/`memory`/
/// `usage` payloads are freed here; the `result` payload is not.
fn handleFrame(
    alloc: std.mem.Allocator,
    f: pipe_proto.Frame,
    response_fd: std.posix.fd_t,
    sink: ActivitySink,
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
    mint_hook: ?MintHook,
    deadline: *i64,
    usage: *pipe_proto.UsageSnapshot,
) ?ReadOutcome {
    switch (f.ftype) {
        .activity => {
            defer alloc.free(f.payload);
            forwardActivity(alloc, sink, f.payload);
        },
        .memory => {
            defer alloc.free(f.payload);
            // Parent POSTs the capture bytes; the frame also attests liveness.
            mem_sink.forward(mem_sink.ctx, f.payload);
        },
        .usage => {
            defer alloc.free(f.payload);
            if (pipe_proto.UsageSnapshot.decode(f.payload)) |snap|
                usage.fold(snap)
            else
                // A malformed 24-byte frame means real wire corruption / version
                // skew (an old child sends NO usage frame, never a bad one), so
                // warn — symmetric with the child-side usage_frame_write_failed.
                log.warn("usage_frame_dropped", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .len = f.payload.len });
        },
        .credential_request => {
            defer alloc.free(f.payload);
            // Mint on the child's behalf and frame the reply back down its stdin.
            // The child is blocked reading that reply, so no stdout frame races.
            serviceCredentialRequest(alloc, f.payload, response_fd, mint_hook);
        },
        .result => return .{ .bytes = f.payload },
        // `lease` / `credential_response` are parent→child only — the parent never
        // reads them off the child's stdout. One here is wire skew; drop it.
        .lease, .credential_response => {
            defer alloc.free(f.payload);
            log.warn("unexpected_child_frame", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .ftype = @tagName(f.ftype) });
        },
    }
    // Every non-terminal frame attests liveness and is a renewal point.
    if (applyTick(renew_hook, deadline, clock.nowMillis(), usage.*)) |reason| return .{ .terminated = true, .terminate_reason = reason };
    return null;
}

/// Parse one `credential_request` payload, mint via the hook, and frame the
/// `credential_response` back to the child's stdin. Best-effort + fail-closed:
/// any parse miss, a null hook, or a broker rejection frames `ok=false`, and the
/// child aborts its tool call. The token (when minted) is owned by `alloc` — freed
/// here right after framing — and is never logged (VLT).
fn serviceCredentialRequest(
    alloc: std.mem.Allocator,
    payload: []const u8,
    response_fd: std.posix.fd_t,
    mint_hook: ?MintHook,
) void {
    const hook = mint_hook orelse return writePipeResponse(alloc, response_fd, .{ .ok = false });
    const parsed = std.json.parseFromSlice(cred.PipeRequest, alloc, payload, .{}) catch
        return writePipeResponse(alloc, response_fd, .{ .ok = false });
    defer parsed.deinit();
    switch (hook.onMint(hook.ctx, alloc, parsed.value.integration, parsed.value.scope)) {
        .minted => |m| {
            defer alloc.free(m.token);
            writePipeResponse(alloc, response_fd, .{ .ok = true, .token = m.token, .expires_at_ms = m.expires_at_ms });
        },
        .rejected => writePipeResponse(alloc, response_fd, .{ .ok = false }),
    }
}

/// Serialize + frame a `credential_response` to the child's stdin. Best-effort:
/// a write failure leaves the child to time out on its read and fail closed (its
/// round-trip is bounded by the lease deadline), so a wedged pipe never hangs the
/// parent. The token, when present, is framed straight through — never logged.
fn writePipeResponse(alloc: std.mem.Allocator, response_fd: std.posix.fd_t, resp: cred.PipeResponse) void {
    const json = std.json.Stringify.valueAlloc(alloc, resp, .{}) catch return;
    defer alloc.free(json);
    pipe_proto.writeFrame(response_fd, .credential_response, json) catch |err|
        log.warn("credential_response_write_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
}

/// Ask the renewal hook for a decision and apply it to `deadline`. Returns true
/// iff the child must be terminated (lease lost / capped / no credits). A null
/// hook (no renewal configured) is a no-op.
/// Run one renewal tick. Returns the class to terminate under, or `null` to keep
/// reading — an optional rather than a bool so the hook's reason survives to
/// `classify` instead of being flattened to "something stopped us".
fn applyTick(renew_hook: ?RenewHook, deadline: *i64, now_ms: i64, usage: pipe_proto.UsageSnapshot) ?types.FailureClass {
    const h = renew_hook orelse return null;
    switch (h.onTick(h.ctx, now_ms, usage)) {
        .keep => {},
        .extend => |new_deadline| deadline.* = new_deadline,
        .terminate => |reason| return reason,
    }
    return null;
}

/// Parse one `activity` frame payload and hand it to the sink. Best-effort: a
/// malformed frame is dropped (activity is cosmetic). The parsed frame's slices
/// borrow `arena`, valid for the synchronous `forward` call.
fn forwardActivity(alloc: std.mem.Allocator, sink: ActivitySink, payload: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const frame = std.json.parseFromSliceLeaky(ActivityFrame, arena.allocator(), payload, .{}) catch return;
    sink.forward(sink.ctx, frame);
}
