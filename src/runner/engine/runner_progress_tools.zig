//! A tool call's frames: its start, its completion, and the call id that pairs
//! them. Split from `runner_progress.zig` at the length cap.
//!
//! NullClaw runs a batch's calls one at a time, firing `tool_call_start` and
//! then `tool_call` for each (`agent/root.zig`), so at most one call is open.
//! Its observer drops the provider's own `tool_call_id`, so the adapter numbers
//! the run's calls itself and stamps that number on every frame of one call.
//! A reader then pairs a call's frames by identity rather than by name and
//! timing, which cannot tell a second call of one tool from a restatement.

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");
const client_errors = @import("client_errors.zig");
const runner_progress = @import("runner_progress.zig");

const Adapter = runner_progress.Adapter;
const ToolCall = @FieldType(nullclaw.observability.ObserverEvent, "tool_call");

const log = logging.scoped(.runner_progress);

/// Decimal digits in the largest call number, `maxInt(u32)`.
const CALL_ID_MAX_DIGITS = 10;

/// The started frame NullClaw's start event can send: it names the tool but
/// not the arguments, which arrive with the completion.
const NO_ARGS = "{}";

/// A call began: it opens under the run's next call number.
pub fn started(self: *Adapter, tool: []const u8) void {
    const number = nextCall(self);
    self.open_call = number;
    var id: [CALL_ID_MAX_DIGITS]u8 = undefined;
    self.writer.write(.{ .tool_call_started = .{
        .name = tool,
        .args_redacted = NO_ARGS,
        .call_id = callId(&id, number),
    } });
}

/// A call finished: its redacted arguments (when NullClaw passed them) and its
/// completion, both under the open call's id, then the context-lifecycle
/// counters. A completion whose start was never reported takes a number of its
/// own, so no two calls ever share an id.
pub fn completed(self: *Adapter, b: ToolCall) void {
    const number = self.open_call orelse nextCall(self);
    self.open_call = null;
    var id: [CALL_ID_MAX_DIGITS]u8 = undefined;
    const call_id = callId(&id, number);
    if (b.args) |raw| {
        // A failed redaction drops the args frame rather than emit `raw`, which
        // may carry a secret; the completed frame below still closes the call.
        if (runner_progress.redactBytes(self.alloc, raw, self.secrets)) |redacted| {
            defer if (redacted.ptr != raw.ptr) self.alloc.free(redacted);
            self.writer.write(.{ .tool_call_started = .{ .name = b.tool, .args_redacted = redacted, .call_id = call_id } });
        } else |err| {
            log.warn("tool_args_redaction_failed_frame_dropped", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        }
    }
    const ms_signed = std.math.cast(i64, b.duration_ms) orelse std.math.maxInt(i64);
    self.writer.write(.{ .tool_call_completed = .{ .name = b.tool, .ms = ms_signed, .call_id = call_id } });
    self.tool_call_count += 1;
    checkpointMemory(self);
    logWindow(self);
}

fn nextCall(self: *Adapter) u32 {
    self.calls_started +%= 1;
    return self.calls_started;
}

fn callId(buf: *[CALL_ID_MAX_DIGITS]u8, number: u32) []const u8 {
    // A u32 prints in at most CALL_ID_MAX_DIGITS digits, so the buffer holds it.
    return std.fmt.bufPrint(buf, "{d}", .{number}) catch unreachable;
}

// L1 nudge: SKILL.md prose tells the fleet to snapshot via memory_store on
// this cadence. The runtime side logs the threshold hit so on-call can confirm
// the prompt is landing, and flushes the in-run store to the parent so a long
// run's learned memory is durable before it finishes (run-end is not the only
// capture point). Best-effort — a blip never disturbs the run.
fn checkpointMemory(self: *Adapter) void {
    if (self.memory_checkpoint_every == 0 or self.tool_call_count % self.memory_checkpoint_every != 0) return;
    self.nudges_emitted += 1;
    log.debug("memory_checkpoint_due", .{
        .tool_count = self.tool_call_count,
        .every = self.memory_checkpoint_every,
        .nudges_emitted = self.nudges_emitted,
    });
    if (self.memory_capturer) |c| c.capture();
}

// L2 window: once the cumulative count crosses the window threshold, every
// subsequent call emits a structured line so on-call can spot a runaway
// incident. The fleet compacts findings via memory_store at the SKILL prose's
// direction — the runtime drops nothing from the conversation itself.
fn logWindow(self: *Adapter) void {
    if (self.tool_window == 0 or self.tool_call_count <= self.tool_window) return;
    self.window_exceeded_logs += 1;
    log.debug("tool_window_exceeded", .{
        .tool_count = self.tool_call_count,
        .window = self.tool_window,
        .excess = self.tool_call_count - self.tool_window,
    });
}
