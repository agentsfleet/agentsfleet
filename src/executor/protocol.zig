//! Length-prefixed JSON-RPC 1.0 protocol for the executor Unix socket.
//!
//! Wire format: [u32 big-endian length][JSON payload]
//! Uses raw posix read/write for socket I/O (Zig 0.15 compatible).

const std = @import("std");
const types = @import("types.zig");

const logging = @import("log");
const log = logging.scoped(.executor_protocol);

const RPC_ID_KEY = "id";
const RPC_METHOD_KEY = "method";
const RPC_PARAMS_KEY = "params";

/// RPC method names — the executor API surface (§2.1).
pub const Method = struct {
    pub const create_execution = "CreateExecution";
    pub const start_stage = "StartStage";
    pub const stream_events = "StreamEvents";
    pub const cancel_execution = "CancelExecution";
    pub const get_usage = "GetUsage";
    pub const destroy_execution = "DestroyExecution";
    pub const heartbeat = "Heartbeat";
    /// M21_001 §1.3
    pub const inject_user_message = "InjectUserMessage";
};

const RpcError = struct {
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

/// Map a FailureClass to its JSON-RPC error code.
pub fn failureToRpcError(failure: ?types.FailureClass) i32 {
    const f = failure orelse return ErrorCode.execution_failed;
    return switch (f) {
        .timeout_kill => ErrorCode.timeout_killed,
        .oom_kill => ErrorCode.oom_killed,
        .policy_deny => ErrorCode.policy_denied,
        .lease_expired => ErrorCode.lease_expired,
        .landlock_deny => ErrorCode.landlock_denied,
        .resource_kill => ErrorCode.resource_killed,
        else => ErrorCode.execution_failed,
    };
}

pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024; // 16 MiB

/// Write a length-prefixed frame to a socket fd.
pub fn writeFrameToFd(fd: std.posix.socket_t, payload: []const u8) !void {
    if (payload.len > MAX_FRAME_SIZE) return error.FrameTooLarge;
    const len: u32 = @intCast(payload.len);
    const len_bytes = std.mem.toBytes(std.mem.nativeTo(u32, len, .big));
    try writeAllFd(fd, &len_bytes);
    try writeAllFd(fd, payload);
}

/// Test-only tap: when set, every successful read of a frame payload is
/// also appended to the recorder list. Single-writer, intended for
/// integration tests that grep the captured byte stream for redaction
/// invariants. Production never assigns this.
pub const ReadByteRecorder = struct {
    alloc: std.mem.Allocator,
    list: *std.ArrayList(u8),
};
pub var read_byte_recorder: ?ReadByteRecorder = null;

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
    if (read_byte_recorder) |rec| {
        rec.list.appendSlice(rec.alloc, buf) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
    }
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
fn writeFrameToBuf(alloc: std.mem.Allocator, buf: *std.ArrayList(u8), payload: []const u8) !void {
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

    try obj.object.put(RPC_ID_KEY,.{ .integer = @intCast(id) });
    try obj.object.put(RPC_METHOD_KEY,.{ .string = method });
    if (params) |p| {
        try obj.object.put(RPC_PARAMS_KEY,p);
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
const ParsedRequest = struct {
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
    const id_val = root.object.get(RPC_ID_KEY) orelse return error.InvalidRequest;
    const id: u64 = switch (id_val) {
        .integer => |i| @intCast(i),
        else => return error.InvalidRequest,
    };
    const method_val = root.object.get(RPC_METHOD_KEY) orelse return error.InvalidRequest;
    const method: []const u8 = switch (method_val) {
        .string => |s| s,
        else => return error.InvalidRequest,
    };
    const params = root.object.get(RPC_PARAMS_KEY);

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
    const id_val = root.object.get(RPC_ID_KEY) orelse return error.InvalidRequest;
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

test {
    _ = @import("protocol_test.zig");
}
