//! Length-prefixed JSON-RPC 1.0 protocol for the executor Unix socket.
//!
//! Wire format: [u32 big-endian length][JSON payload]
//! Uses raw posix read/write for socket I/O (Zig 0.15 compatible).

const std = @import("std");

/// RPC method names — the executor API surface (§2.1).
pub const Method = struct {
    pub const create_execution = "CreateExecution";
    pub const start_stage = "StartStage";
    pub const stream_events = "StreamEvents";
    pub const cancel_execution = "CancelExecution";
    pub const get_usage = "GetUsage";
    pub const destroy_execution = "DestroyExecution";
    pub const heartbeat = "Heartbeat";
};

pub const RpcError = struct {
    code: i32,
    message: []const u8,
};

/// Standard RPC error codes.
pub const ErrorCode = struct {
    pub const parse_error: i32 = -32700;
    pub const invalid_request: i32 = -32600;
    pub const method_not_found: i32 = -32601;
    pub const invalid_params: i32 = -32602;
    pub const internal_error: i32 = -32603;
    pub const execution_failed: i32 = -1;
    pub const timeout_killed: i32 = -2;
    pub const oom_killed: i32 = -3;
    pub const policy_denied: i32 = -4;
    pub const lease_expired: i32 = -5;
    pub const landlock_denied: i32 = -6;
    pub const resource_killed: i32 = -7;
};

pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024; // 16 MiB

/// Write a length-prefixed frame to a socket fd.
pub fn writeFrameToFd(fd: std.posix.socket_t, payload: []const u8) !void {
    if (payload.len > MAX_FRAME_SIZE) return error.FrameTooLarge;
    const len: u32 = @intCast(payload.len);
    const len_bytes = std.mem.toBytes(std.mem.nativeTo(u32, len, .big));
    try writeAllFd(fd, &len_bytes);
    try writeAllFd(fd, payload);
}

/// Read a length-prefixed frame from a socket fd.
/// Caller owns the returned slice.
pub fn readFrameFromFd(alloc: std.mem.Allocator, fd: std.posix.socket_t) ![]u8 {
    var len_bytes: [4]u8 = undefined;
    readAllFd(fd, &len_bytes) catch return error.ConnectionClosed;
    const len = std.mem.bigToNative(u32, std.mem.bytesToValue(u32, &len_bytes));
    if (len > MAX_FRAME_SIZE) return error.FrameTooLarge;
    const buf = try alloc.alloc(u8, len);
    errdefer alloc.free(buf);
    readAllFd(fd, buf) catch return error.ConnectionClosed;
    return buf;
}

fn writeAllFd(fd: std.posix.socket_t, data: []const u8) !void {
    var offset: usize = 0;
    while (offset < data.len) {
        const n = std.posix.write(fd, data[offset..]) catch return error.BrokenPipe;
        offset += n;
    }
}

fn readAllFd(fd: std.posix.socket_t, buf: []u8) !void {
    var offset: usize = 0;
    while (offset < buf.len) {
        const n = std.posix.read(fd, buf[offset..]) catch return error.ConnectionClosed;
        if (n == 0) return error.ConnectionClosed;
        offset += n;
    }
}

/// Write a length-prefixed frame to an ArrayList writer (for tests/serialization).
pub fn writeFrameToBuf(alloc: std.mem.Allocator, buf: *std.ArrayList(u8), payload: []const u8) !void {
    if (payload.len > MAX_FRAME_SIZE) return error.FrameTooLarge;
    const len: u32 = @intCast(payload.len);
    const len_bytes = std.mem.toBytes(std.mem.nativeTo(u32, len, .big));
    try buf.appendSlice(alloc, &len_bytes);
    try buf.appendSlice(alloc, payload);
}

/// Serialize a JSON-RPC request to a buffer.
pub fn serializeRequest(alloc: std.mem.Allocator, id: u64, method: []const u8, params: ?std.json.Value) ![]u8 {
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();

    try obj.object.put("id", .{ .integer = @intCast(id) });
    try obj.object.put("method", .{ .string = method });
    if (params) |p| {
        try obj.object.put("params", p);
    }

    return std.json.Stringify.valueAlloc(alloc, obj, .{});
}

/// Send a JSON-RPC request over a socket fd.
pub fn sendRequest(alloc: std.mem.Allocator, fd: std.posix.socket_t, id: u64, method: []const u8, params: ?std.json.Value) !void {
    const serialized = try serializeRequest(alloc, id, method, params);
    defer alloc.free(serialized);
    try writeFrameToFd(fd, serialized);
}

/// Parse a JSON-RPC request from a frame payload.
pub const ParsedRequest = struct {
    id: u64,
    method: []const u8,
    params: ?std.json.Value,
    parsed: std.json.Parsed(std.json.Value),

    pub fn deinit(self: *ParsedRequest) void {
        self.parsed.deinit();
    }
};

pub fn parseRequest(alloc: std.mem.Allocator, payload: []const u8) !ParsedRequest {
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, payload, .{});
    errdefer parsed.deinit();

    const root = parsed.value;
    const id_val = root.object.get("id") orelse return error.InvalidRequest;
    const id: u64 = switch (id_val) {
        .integer => |i| @intCast(i),
        else => return error.InvalidRequest,
    };
    const method_val = root.object.get("method") orelse return error.InvalidRequest;
    const method: []const u8 = switch (method_val) {
        .string => |s| s,
        else => return error.InvalidRequest,
    };
    const params = root.object.get("params");

    return .{
        .id = id,
        .method = method,
        .params = params,
        .parsed = parsed,
    };
}

/// Parse a JSON-RPC response from a frame payload.
pub const ParsedResponse = struct {
    id: u64,
    result: ?std.json.Value,
    rpc_error: ?RpcError,
    parsed: std.json.Parsed(std.json.Value),

    pub fn deinit(self: *ParsedResponse) void {
        self.parsed.deinit();
    }
};

pub fn parseResponse(alloc: std.mem.Allocator, payload: []const u8) !ParsedResponse {
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, payload, .{});
    errdefer parsed.deinit();

    const root = parsed.value;
    const id_val = root.object.get("id") orelse return error.InvalidRequest;
    const id: u64 = switch (id_val) {
        .integer => |i| @intCast(i),
        else => return error.InvalidRequest,
    };
    const result = root.object.get("result");
    var rpc_error: ?RpcError = null;
    if (root.object.get("error")) |err_val| {
        if (err_val == .object) {
            const code_val = err_val.object.get("code") orelse return error.InvalidRequest;
            const msg_val = err_val.object.get("message") orelse return error.InvalidRequest;
            rpc_error = .{
                .code = switch (code_val) {
                    .integer => |i| @intCast(i),
                    else => return error.InvalidRequest,
                },
                .message = switch (msg_val) {
                    .string => |s| s,
                    else => return error.InvalidRequest,
                },
            };
        }
    }

    return .{
        .id = id,
        .result = result,
        .rpc_error = rpc_error,
        .parsed = parsed,
    };
}

test "serializeRequest and parseRequest round trip" {
    const alloc = std.testing.allocator;
    const serialized = try serializeRequest(alloc, 42, Method.create_execution, null);
    defer alloc.free(serialized);

    var req = try parseRequest(alloc, serialized);
    defer req.deinit();
    try std.testing.expectEqual(@as(u64, 42), req.id);
    try std.testing.expectEqualStrings(Method.create_execution, req.method);
}

test "serializeRequest with params round trips" {
    const alloc = std.testing.allocator;
    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/test" });

    const serialized = try serializeRequest(alloc, 1, Method.create_execution, params);
    defer alloc.free(serialized);

    var req = try parseRequest(alloc, serialized);
    defer req.deinit();
    try std.testing.expectEqual(@as(u64, 1), req.id);
    try std.testing.expect(req.params != null);
}

test "parseRequest rejects malformed JSON" {
    const alloc = std.testing.allocator;
    const result = parseRequest(alloc, "not json at all{{{");
    try std.testing.expectError(error.SyntaxError, result);
}

test "parseRequest rejects missing id" {
    const alloc = std.testing.allocator;
    const result = parseRequest(alloc, "{\"method\":\"Heartbeat\"}");
    try std.testing.expectError(error.InvalidRequest, result);
}

test "parseRequest rejects missing method" {
    const alloc = std.testing.allocator;
    const result = parseRequest(alloc, "{\"id\":1}");
    try std.testing.expectError(error.InvalidRequest, result);
}

test "parseResponse parses success response" {
    const alloc = std.testing.allocator;
    const payload = "{\"id\":1,\"result\":true}";
    var resp = try parseResponse(alloc, payload);
    defer resp.deinit();
    try std.testing.expectEqual(@as(u64, 1), resp.id);
    try std.testing.expect(resp.result != null);
    try std.testing.expect(resp.rpc_error == null);
}

test "parseResponse parses error response" {
    const alloc = std.testing.allocator;
    const payload = "{\"id\":2,\"error\":{\"code\":-32601,\"message\":\"Unknown method\"}}";
    var resp = try parseResponse(alloc, payload);
    defer resp.deinit();
    try std.testing.expectEqual(@as(u64, 2), resp.id);
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, ErrorCode.method_not_found), resp.rpc_error.?.code);
    try std.testing.expectEqualStrings("Unknown method", resp.rpc_error.?.message);
}

test "parseResponse rejects missing id" {
    const alloc = std.testing.allocator;
    const result = parseResponse(alloc, "{\"result\":true}");
    try std.testing.expectError(error.InvalidRequest, result);
}

// ── T7: Regression — error code values are pinned ────────────────────
test "ErrorCode values match JSON-RPC spec" {
    // Standard JSON-RPC 2.0 error codes.
    try std.testing.expectEqual(@as(i32, -32700), ErrorCode.parse_error);
    try std.testing.expectEqual(@as(i32, -32600), ErrorCode.invalid_request);
    try std.testing.expectEqual(@as(i32, -32601), ErrorCode.method_not_found);
    try std.testing.expectEqual(@as(i32, -32602), ErrorCode.invalid_params);
    try std.testing.expectEqual(@as(i32, -32603), ErrorCode.internal_error);
    // Application-specific codes.
    try std.testing.expectEqual(@as(i32, -1), ErrorCode.execution_failed);
    try std.testing.expectEqual(@as(i32, -2), ErrorCode.timeout_killed);
    try std.testing.expectEqual(@as(i32, -3), ErrorCode.oom_killed);
    try std.testing.expectEqual(@as(i32, -4), ErrorCode.policy_denied);
    try std.testing.expectEqual(@as(i32, -5), ErrorCode.lease_expired);
    try std.testing.expectEqual(@as(i32, -6), ErrorCode.landlock_denied);
    try std.testing.expectEqual(@as(i32, -7), ErrorCode.resource_killed);
}

// ── T7: Regression — Method name constants are stable ────────────────
test "Method constants match expected strings" {
    try std.testing.expectEqualStrings("CreateExecution", Method.create_execution);
    try std.testing.expectEqualStrings("StartStage", Method.start_stage);
    try std.testing.expectEqualStrings("StreamEvents", Method.stream_events);
    try std.testing.expectEqualStrings("CancelExecution", Method.cancel_execution);
    try std.testing.expectEqualStrings("GetUsage", Method.get_usage);
    try std.testing.expectEqualStrings("DestroyExecution", Method.destroy_execution);
    try std.testing.expectEqualStrings("Heartbeat", Method.heartbeat);
}

// ── T8: Security — MAX_FRAME_SIZE is pinned ──────────────────────────
test "MAX_FRAME_SIZE is 16 MiB" {
    try std.testing.expectEqual(@as(u32, 16 * 1024 * 1024), MAX_FRAME_SIZE);
}

// ── T2: Edge case — parseRequest with empty string ───────────────────
test "parseRequest rejects empty payload" {
    const alloc = std.testing.allocator;
    const result = parseRequest(alloc, "");
    try std.testing.expectError(error.UnexpectedEndOfInput, result);
}

// ── T2: Edge case — parseResponse with only whitespace ───────────────
test "parseResponse rejects whitespace-only payload" {
    const alloc = std.testing.allocator;
    const result = parseResponse(alloc, "   ");
    try std.testing.expectError(error.UnexpectedEndOfInput, result);
}
