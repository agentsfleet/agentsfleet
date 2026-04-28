//! Best-effort PUBLISH helpers for the live-tail activity channel.
//!
//! Channel: `zombie:{id}:activity`. Single publisher (the worker that
//! owns the zombie's event loop), zero-or-N subscribers (SSE handlers).
//! Ephemeral — Redis pub/sub has no buffer or replay; if a frame drops
//! the operator backfills via `GET /events?cursor=...`.
//!
//! Every helper is best-effort: PUBLISH failure is logged at debug
//! and swallowed. The durable record (zombie_events + telemetry +
//! sessions) is independent of pub/sub by design (Invariant 7).

const channel_prefix = "zombie:";
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

fn buildChannel(buf: []u8, zombie_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{ channel_prefix, zombie_id, channel_suffix });
}

/// Publish a pre-encoded JSON payload to the activity channel.
/// Internal — the kind-specific helpers below build the payload and
/// dispatch through here. Best-effort.
fn publishRaw(client: *Client, alloc: Allocator, zombie_id: []const u8, payload: []const u8) void {
    var channel_buf: [128]u8 = undefined;
    const channel = buildChannel(&channel_buf, zombie_id) catch |err| {
        log.debug("activity.channel_format_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return;
    };
    _ = alloc;
    client.publish(channel, payload) catch |err| {
        log.debug("activity.publish_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
    };
}

pub fn publishEventReceived(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    actor: []const u8,
) void {
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_EVENT_RECEIVED,
        .event_id = event_id,
        .actor = actor,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_EVENT_RECEIVED, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

pub fn publishToolCallStarted(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    args_redacted_json: []const u8,
) void {
    // args_redacted is opaque pre-stringified JSON from the executor
    // (built sandbox-side after secret substitution). We re-emit it
    // verbatim inside the frame via std.json.Value.
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, args_redacted_json, .{}) catch |err| {
        log.debug("activity.args_parse_fail err={s}", .{@errorName(err)});
        return;
    };
    defer parsed.deinit();
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_TOOL_CALL_STARTED,
        .event_id = event_id,
        .name = name,
        .args_redacted = parsed.value,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_TOOL_CALL_STARTED, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

pub fn publishToolCallProgress(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    elapsed_ms: i64,
) void {
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_TOOL_CALL_PROGRESS,
        .event_id = event_id,
        .name = name,
        .elapsed_ms = elapsed_ms,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_TOOL_CALL_PROGRESS, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

pub fn publishChunk(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    text: []const u8,
) void {
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_CHUNK,
        .event_id = event_id,
        .text = text,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_CHUNK, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

pub fn publishToolCallCompleted(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    name: []const u8,
    ms: i64,
) void {
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_TOOL_CALL_COMPLETED,
        .event_id = event_id,
        .name = name,
        .ms = ms,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_TOOL_CALL_COMPLETED, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

pub fn publishEventComplete(
    client: *Client,
    alloc: Allocator,
    zombie_id: []const u8,
    event_id: []const u8,
    status: []const u8,
) void {
    const payload = std.json.Stringify.valueAlloc(alloc, .{
        .kind = KIND_EVENT_COMPLETE,
        .event_id = event_id,
        .status = status,
    }, .{}) catch |err| {
        log.debug("activity.encode_fail kind={s} err={s}", .{ KIND_EVENT_COMPLETE, @errorName(err) });
        return;
    };
    defer alloc.free(payload);
    publishRaw(client, alloc, zombie_id, payload);
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const Client = @import("../queue/redis_client.zig").Client;
const log = std.log.scoped(.activity_publisher);
