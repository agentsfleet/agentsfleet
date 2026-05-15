//! Progress-frame protocol primitives for the executor RPC.
//!
//! The executor RPC was previously a single request → single response. To
//! drive the live-tail UX (`zombie:{id}:activity` SSE) the worker needs
//! visibility into what the agent is doing inside the sandbox — tool
//! calls starting and completing, response tokens streaming, long tool
//! calls heart-beating so the UI can keep its spinner alive.
//!
//! Wire shape: same Unix socket, same length-prefixed JSON framing as
//! today (see `protocol.zig`). The reply for a `StartStage` request is
//! now multiplexed: zero-or-more `ProgressFrame` frames followed by
//! exactly one terminal `result`/`error` JSON-RPC response. The id of
//! every progress frame matches the StartStage request id.
//!
//! Why version-bump the RPC? An old worker reading multiplexed frames
//! would mis-parse the first progress frame as the final response. The
//! `Hello` handshake fires on socket connect with `rpc_version: 2` so
//! mismatched peers fail fast instead of producing garbage.
//!
//! This file ships the type/encode/decode primitives only. Worker-side
//! streaming reader and executor-side emit hooks land in subsequent
//! slices; until those land, no progress frame is ever born and the
//! activity channel carries `event_received` + `event_complete` only.

const std = @import("std");
const Allocator = std.mem.Allocator;

const S_TOOL_CALL_STARTED = "tool_call_started";
const S_TOOL_CALL_COMPLETED = "tool_call_completed";
const S_N_2_0 = "2.0";
const S_AGENT_RESPONSE_CHUNK = "agent_response_chunk";
const S_NAME = "name";
const S_TOOL_CALL_PROGRESS = "tool_call_progress";

pub const rpc_version: u32 = 2;

/// Sentinel JSON-RPC method name carried on progress frames so the
/// worker's read loop can distinguish them from terminal responses.
/// Terminal responses carry no `method` field (they have `result` or
/// `error`); progress frames are JSON-RPC notifications with this
/// method and the frame body in `params`.
pub const progress_method = "Progress";

pub const FrameKind = enum {
    tool_call_started,
    agent_response_chunk,
    tool_call_completed,
    tool_call_progress,

    pub fn toSlice(self: FrameKind) []const u8 {
        return switch (self) {
            .tool_call_started => S_TOOL_CALL_STARTED,
            .agent_response_chunk => S_AGENT_RESPONSE_CHUNK,
            .tool_call_completed => S_TOOL_CALL_COMPLETED,
            .tool_call_progress => S_TOOL_CALL_PROGRESS,
        };
    }

    pub fn fromSlice(s: []const u8) ?FrameKind {
        if (std.mem.eql(u8, s, S_TOOL_CALL_STARTED)) return .tool_call_started;
        if (std.mem.eql(u8, s, S_AGENT_RESPONSE_CHUNK)) return .agent_response_chunk;
        if (std.mem.eql(u8, s, S_TOOL_CALL_COMPLETED)) return .tool_call_completed;
        if (std.mem.eql(u8, s, S_TOOL_CALL_PROGRESS)) return .tool_call_progress;
        return null;
    }
};

/// Body of a progress frame. The `event_id` identifying the zombie
/// event is NOT carried here; the worker correlates by RPC request id
/// (StartStage → progress frames sharing that id).
///
/// `args_redacted` is opaque, pre-stringified JSON built inside the
/// executor after substituting `${secrets.x.y}` placeholders. The
/// resolved secret bytes never cross the RPC boundary on this channel.
pub const ProgressFrame = union(FrameKind) {
    tool_call_started: ToolCallStarted,
    agent_response_chunk: AgentResponseChunk,
    tool_call_completed: ToolCallCompleted,
    tool_call_progress: ToolCallProgress,

    pub const ToolCallStarted = struct {
        name: []const u8,
        args_redacted: []const u8, // opaque JSON
    };
    pub const AgentResponseChunk = struct {
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

    pub fn kind(self: ProgressFrame) FrameKind {
        return std.meta.activeTag(self);
    }
};

/// HELLO frame exchanged on socket connect (both directions). Mismatch
/// → fast-fail with `executor.rpc_version_mismatch`. Pre-v2.0.0
/// teardown era — no v1 compat shim, fail loud and fail fast.
pub const Hello = struct {
    rpc_version: u32,
};

/// Encode a progress frame as a JSON-RPC notification payload (the
/// length-prefix layer is the caller's responsibility — usually
/// `protocol.writeFrameToFd` after the bytes are formed).
///
/// Caller owns the returned slice.
pub fn encodeProgress(alloc: Allocator, request_id: u64, frame: ProgressFrame) ![]u8 {
    return switch (frame) {
        .tool_call_started => |body| try std.json.Stringify.valueAlloc(alloc, .{
            .jsonrpc = S_N_2_0,
            .id = request_id,
            .method = progress_method,
            .params = .{
                .kind = S_TOOL_CALL_STARTED,
                .name = body.name,
                .args_redacted_json = body.args_redacted,
            },
        }, .{}),
        .agent_response_chunk => |body| try std.json.Stringify.valueAlloc(alloc, .{
            .jsonrpc = S_N_2_0,
            .id = request_id,
            .method = progress_method,
            .params = .{
                .kind = S_AGENT_RESPONSE_CHUNK,
                .text = body.text,
            },
        }, .{}),
        .tool_call_completed => |body| try std.json.Stringify.valueAlloc(alloc, .{
            .jsonrpc = S_N_2_0,
            .id = request_id,
            .method = progress_method,
            .params = .{
                .kind = S_TOOL_CALL_COMPLETED,
                .name = body.name,
                .ms = body.ms,
            },
        }, .{}),
        .tool_call_progress => |body| try std.json.Stringify.valueAlloc(alloc, .{
            .jsonrpc = S_N_2_0,
            .id = request_id,
            .method = progress_method,
            .params = .{
                .kind = S_TOOL_CALL_PROGRESS,
                .name = body.name,
                .elapsed_ms = body.elapsed_ms,
            },
        }, .{}),
    };
}

/// Detect whether a JSON payload is a progress frame (`method ==
/// "Progress"`) without fully decoding. Used by the worker's read loop
/// to fork between progress dispatch and terminal-response handling.
pub fn isProgressPayload(parsed_root: std.json.Value) bool {
    if (parsed_root != .object) return false;
    const method_v = parsed_root.object.get("method") orelse return false;
    if (method_v != .string) return false;
    return std.mem.eql(u8, method_v.string, progress_method);
}

/// Frame + id pair extracted from an already-parsed JSON value. Borrowed
/// slices point into the caller-owned `std.json.Parsed` value.
pub const ProgressDecoded = struct {
    request_id: u64,
    frame: ProgressFrame,
};

/// Decode a progress frame from an already-parsed JSON value. The
/// returned frame's string slices point into the caller's parsed value;
/// caller is responsible for the parsed value's lifetime. Used by
/// `transport.sendRequestStreaming` to avoid a second parse after the
/// progress-vs-terminal discrimination.
pub fn decodeProgressFromValue(root: std.json.Value) !ProgressDecoded {
    if (root != .object) return error.InvalidProgressFrame;

    const id_v = root.object.get("id") orelse return error.InvalidProgressFrame;
    const request_id: u64 = switch (id_v) {
        .integer => |i| if (i < 0) return error.InvalidProgressFrame else @intCast(i),
        else => return error.InvalidProgressFrame,
    };

    const params_v = root.object.get("params") orelse return error.InvalidProgressFrame;
    if (params_v != .object) return error.InvalidProgressFrame;

    const kind_v = params_v.object.get("kind") orelse return error.InvalidProgressFrame;
    if (kind_v != .string) return error.InvalidProgressFrame;
    const fk = FrameKind.fromSlice(kind_v.string) orelse return error.UnknownFrameKind;

    const frame: ProgressFrame = switch (fk) {
        .tool_call_started => .{ .tool_call_started = .{
            .name = strField(params_v, S_NAME) orelse return error.InvalidProgressFrame,
            .args_redacted = strField(params_v, "args_redacted_json") orelse "{}",
        } },
        .agent_response_chunk => .{ .agent_response_chunk = .{
            .text = strField(params_v, "text") orelse return error.InvalidProgressFrame,
        } },
        .tool_call_completed => .{ .tool_call_completed = .{
            .name = strField(params_v, S_NAME) orelse return error.InvalidProgressFrame,
            .ms = intField(params_v, "ms") orelse return error.InvalidProgressFrame,
        } },
        .tool_call_progress => .{ .tool_call_progress = .{
            .name = strField(params_v, S_NAME) orelse return error.InvalidProgressFrame,
            .elapsed_ms = intField(params_v, "elapsed_ms") orelse return error.InvalidProgressFrame,
        } },
    };

    return .{ .request_id = request_id, .frame = frame };
}

pub fn encodeHello(alloc: Allocator, hello: Hello) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .hello = .{ .rpc_version = hello.rpc_version },
    }, .{});
}

pub fn decodeHello(alloc: Allocator, payload: []const u8) !Hello {
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, payload, .{});
    defer parsed.deinit();
    const root = parsed.value;
    if (root != .object) return error.InvalidHello;
    const hello_v = root.object.get("hello") orelse return error.InvalidHello;
    if (hello_v != .object) return error.InvalidHello;
    const rv = hello_v.object.get("rpc_version") orelse return error.InvalidHello;
    const v: u32 = switch (rv) {
        .integer => |i| if (i < 0) return error.InvalidHello else @intCast(i),
        else => return error.InvalidHello,
    };
    return .{ .rpc_version = v };
}

fn strField(obj: std.json.Value, name: []const u8) ?[]const u8 {
    const v = obj.object.get(name) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

fn intField(obj: std.json.Value, name: []const u8) ?i64 {
    const v = obj.object.get(name) orelse return null;
    return switch (v) {
        .integer => |i| i,
        else => null,
    };
}
