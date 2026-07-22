//! Best-effort PUBLISH helpers for the live-tail activity channel.
//!
//! Channel: `fleet:{id}:activity`. Single publisher (the worker that
//! owns the fleet's event loop), zero-or-N subscribers (SSE handlers).
//! Ephemeral — Redis pub/sub has no buffer or replay; if a frame drops
//! the operator backfills via `GET /events?cursor=...`.
//!
//! Every helper is best-effort: PUBLISH failure is logged at debug
//! and swallowed. The durable record (fleet_events + telemetry +
//! sessions) is independent of pub/sub by design (Invariant 7).
//!
//! Allocator discipline — per-frame alloc is the worker's hot path. The
//! caller owns a `Scratch` (reusable Allocating writer); each helper
//! `clearRetainingCapacity()`s it, encodes via the Writer interface,
//! and PUBLISHes the borrowed slice. After warmup the buffer holds the
//! peak frame size and the steady-state alloc count is zero.

const channel_prefix = "fleet:";
const channel_suffix = ":activity";

// Frame kind discriminators carried as the `kind` field on every payload.
// The dashboard's LiveFrame union (ui/packages/app/lib/api/events.ts)
// keeps the same set of strings; touch both sides together.
pub const KIND_EVENT_RECEIVED = "event_received";
pub const KIND_TOOL_CALL_STARTED = "tool_call_started";
pub const KIND_TOOL_CALL_PROGRESS = "tool_call_progress";
pub const KIND_CHUNK = "chunk";
pub const KIND_TOOL_CALL_COMPLETED = "tool_call_completed";
pub const KIND_EVENT_COMPLETE = "event_complete";

// Synthetic install-progression frames. The create path emits these on a
// deferred tick after the 201 so the post-create SSE subscriber catches the
// fleet crossing each step; the dashboard advances its rendered install steps
// off them and flips the fleet from installing→active on `install:ready`.
// The pub/sub channel is ephemeral (no replay) — a subscriber that connects
// late reconciles from `core.fleets.status` instead. Mirror these verbatim in
// the dashboard's LiveFrame union (ui/packages/app/lib/api/events.ts).
pub const KIND_INSTALL_CREATING = "install:creating";
pub const KIND_INSTALL_PROVISIONING = "install:provisioning";
pub const KIND_INSTALL_READY = "install:ready";
pub const KIND_INSTALL_ERROR = "install:error";

/// Reusable encode buffer. One per worker / per event lifetime — the
/// single-publisher invariant means callers never share these across
/// threads. Construct with `std.Io.Writer.Allocating.init(alloc)` and
/// `defer scratch.deinit()`.
pub const Scratch = std.Io.Writer.Allocating;

fn buildChannel(buf: []u8, fleet_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{ channel_prefix, fleet_id, channel_suffix });
}

fn publishRaw(client: *Client, fleet_id: []const u8, payload: []const u8) void {
    var channel_buf: [128]u8 = undefined;
    const channel = buildChannel(&channel_buf, fleet_id) catch |err| {
        log.debug("channel_format_fail", .{ .fleet_id = fleet_id, .err = @errorName(err) });
        return;
    };
    client.publish(channel, payload) catch |err| {
        log.debug("publish_fail", .{ .fleet_id = fleet_id, .err = @errorName(err) });
    };
}

fn encodeAndPublish(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    kind: []const u8,
    payload: anytype,
) void {
    scratch.clearRetainingCapacity();
    std.json.Stringify.value(payload, .{}, &scratch.writer) catch |err| {
        log.debug("encode_fail", .{ .kind = kind, .err = @errorName(err) });
        return;
    };
    publishRaw(client, fleet_id, scratch.written());
}

pub fn publishEventReceived(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    event_id: []const u8,
    actor: []const u8,
) void {
    encodeAndPublish(client, scratch, fleet_id, KIND_EVENT_RECEIVED, .{
        .kind = KIND_EVENT_RECEIVED,
        .event_id = event_id,
        .actor = actor,
    });
}

pub fn publishToolCallStarted(
    client: *Client,
    scratch: *Scratch,
    alloc: Allocator,
    fleet_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    args_redacted_json: []const u8,
) void {
    // args_redacted is opaque pre-stringified JSON from the runner
    // (built sandbox-side after secret substitution). Re-emit it
    // verbatim inside the frame via std.json.Value. The parse is
    // upstream of the encode; tool_call_started fires once per tool
    // invocation, not per token, so the per-call alloc is acceptable.
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, args_redacted_json, .{}) catch |err| {
        log.debug("args_parse_fail", .{ .err = @errorName(err) });
        return;
    };
    defer parsed.deinit();
    encodeAndPublish(client, scratch, fleet_id, KIND_TOOL_CALL_STARTED, .{
        .kind = KIND_TOOL_CALL_STARTED,
        .event_id = event_id,
        .name = name,
        .args_redacted = parsed.value,
    });
}

pub fn publishToolCallProgress(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    elapsed_ms: i64,
) void {
    encodeAndPublish(client, scratch, fleet_id, KIND_TOOL_CALL_PROGRESS, .{
        .kind = KIND_TOOL_CALL_PROGRESS,
        .event_id = event_id,
        .name = name,
        .elapsed_ms = elapsed_ms,
    });
}

pub fn publishChunk(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    event_id: []const u8,
    text: []const u8,
) void {
    encodeAndPublish(client, scratch, fleet_id, KIND_CHUNK, .{
        .kind = KIND_CHUNK,
        .event_id = event_id,
        .text = text,
    });
}

pub fn publishToolCallCompleted(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    ms: i64,
) void {
    encodeAndPublish(client, scratch, fleet_id, KIND_TOOL_CALL_COMPLETED, .{
        .kind = KIND_TOOL_CALL_COMPLETED,
        .event_id = event_id,
        .name = name,
        .ms = ms,
    });
}

/// Why an event completed badly, carried on the completion frame. Both fields
/// empty on a clean finish — the dashboard reads empty as absent. Named fields
/// rather than adjacent same-typed arguments, so a swapped pair cannot compile.
pub const FailureCause = struct {
    label: []const u8 = "",
    detail: []const u8 = "",
};

pub fn publishEventComplete(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    event_id: []const u8,
    status: []const u8,
    cause: FailureCause,
) void {
    encodeAndPublish(client, scratch, fleet_id, KIND_EVENT_COMPLETE, .{
        .kind = KIND_EVENT_COMPLETE,
        .event_id = event_id,
        .status = status,
        .failure_label = cause.label,
        .failure_detail = cause.detail,
    });
}

/// Publish one synthetic install step. `kind` is one of the `KIND_INSTALL_*`
/// discriminators — the kind itself names the step, so the frame body is just
/// `{ kind }` (the dashboard switches on it). Best-effort like every helper:
/// a dropped frame is reconciled from `core.fleets.status` on (re)connect.
pub fn publishInstallStep(
    client: *Client,
    scratch: *Scratch,
    fleet_id: []const u8,
    kind: []const u8,
) void {
    encodeAndPublish(client, scratch, fleet_id, kind, .{ .kind = kind });
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const Client = @import("../queue/redis_client.zig").Client;
const logging = @import("log");

const log = logging.scoped(.activity_publisher);
