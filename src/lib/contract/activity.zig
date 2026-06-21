//! activity.zig — the live-tail activity wire frame for the `activity` verb.
//!
//! `POST /v1/runners/me/leases/{lease_id}/activity` carries an `ActivityRequest`
//! (a batch of `ActivityFrame`s). The runner builds frames from NullClaw observer
//! events inside the sandboxed child, streams them to its parent over the lease
//! pipe, and the parent forwards them to `agentsfleetd`, which `PUBLISH`es each to
//! `fleet:{id}:activity` for the Server-Sent-Events (SSE) live tail.
//!
//! Two planes, kept apart on purpose: activity is ephemeral + best-effort (a
//! dropped frame is cosmetic), while `report` is the durable system of record.
//! Args are redacted runner-side before they reach this type — the resolved
//! secret bytes never cross this boundary (`args_redacted` is opaque JSON).
//!
//! The union tag names ARE the wire `kind` discriminators (std.json renders a
//! `union(enum)` as `{"<tag>": value}`), so the enum is the single source (RULE
//! UFS) — there are no re-spelled `kind` string literals. The agentsfleetd handler
//! switches on the variant and maps each to an `activity_publisher` SSE frame;
//! that handler is the only place the runner-wire vocabulary and the SSE/UI
//! vocabulary (`activity_publisher.KIND_*`, mirrored in `events.ts`) meet.

const std = @import("std");

/// One progress frame. Mirrors the NullClaw observer events the runner cares
/// about for the live tail. `tool_call_progress` is a long-tool heartbeat so the
/// UI spinner survives a slow tool.
pub const ActivityFrame = union(enum) {
    tool_call_started: ToolCallStarted,
    fleet_response_chunk: FleetResponseChunk,
    tool_call_completed: ToolCallCompleted,
    tool_call_progress: ToolCallProgress,

    /// `args_redacted` is opaque, pre-stringified JSON built runner-side after
    /// `${secrets.x.y}` substitution — never the resolved bytes.
    pub const ToolCallStarted = struct {
        name: []const u8,
        args_redacted: []const u8,
    };
    pub const FleetResponseChunk = struct {
        text: []const u8,
    };
    pub const ToolCallCompleted = struct {
        name: []const u8,
        ms: i64,
    };
    pub const ToolCallProgress = struct {
        name: []const u8,
        elapsed_ms: i64,
    };
};

/// Request body for the `activity` verb — a batch of frames. The runner forwards
/// frames as they stream off the pipe (one per request today; the array shape
/// lets a later slice coalesce without a wire change). Response is 202, no ack.
pub const ActivityRequest = struct {
    frames: []const ActivityFrame,
};

// ── Tests ───────────────────────────────────────────────────────────────────

test "ActivityFrame round-trips through std.json as a tagged union" {
    const alloc = std.testing.allocator;
    const original = ActivityFrame{ .tool_call_started = .{ .name = "fly_deploy", .args_redacted = "{\"app\":\"x\"}" } };

    const wire = try std.json.Stringify.valueAlloc(alloc, original, .{});
    defer alloc.free(wire);

    const parsed = try std.json.parseFromSlice(ActivityFrame, alloc, wire, .{});
    defer parsed.deinit();

    try std.testing.expect(parsed.value == .tool_call_started);
    try std.testing.expectEqualStrings("fly_deploy", parsed.value.tool_call_started.name);
    try std.testing.expectEqualStrings("{\"app\":\"x\"}", parsed.value.tool_call_started.args_redacted);
}

test "ActivityRequest carries a batch of mixed frames" {
    const alloc = std.testing.allocator;
    const frames = [_]ActivityFrame{
        .{ .fleet_response_chunk = .{ .text = "thinking" } },
        .{ .tool_call_completed = .{ .name = "fly_status", .ms = 8210 } },
        .{ .tool_call_progress = .{ .name = "deploy", .elapsed_ms = 4000 } },
    };
    const req = ActivityRequest{ .frames = &frames };

    const wire = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(wire);

    const parsed = try std.json.parseFromSlice(ActivityRequest, alloc, wire, .{});
    defer parsed.deinit();

    try std.testing.expectEqual(@as(usize, 3), parsed.value.frames.len);
    try std.testing.expect(parsed.value.frames[0] == .fleet_response_chunk);
    try std.testing.expectEqual(@as(i64, 8210), parsed.value.frames[1].tool_call_completed.ms);
    try std.testing.expectEqual(@as(i64, 4000), parsed.value.frames[2].tool_call_progress.elapsed_ms);
}
