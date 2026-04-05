//! M22_001 §1: SSE streaming for `zombied run --watch`.
//!
//! Connects to GET /v1/runs/{id}:stream, buffers the SSE response,
//! then parses and renders gate_result events to stdout.

const std = @import("std");

/// SSE line parser state. Buffers bytes until '\n', dispatches events on blank lines.
pub const SseState = struct {
    alloc: std.mem.Allocator,
    line_buf: std.ArrayList(u8),
    current_data: ?[]u8,
    current_event: ?[]u8,
    done: bool,

    pub fn init(alloc: std.mem.Allocator) SseState {
        return .{
            .alloc = alloc,
            .line_buf = .{},
            .current_data = null,
            .current_event = null,
            .done = false,
        };
    }

    pub fn deinit(self: *SseState) void {
        self.line_buf.deinit(self.alloc);
        if (self.current_data) |d| self.alloc.free(d);
        if (self.current_event) |e| self.alloc.free(e);
    }

    fn processLine(self: *SseState, raw: []const u8) void {
        const trimmed = std.mem.trimRight(u8, raw, "\r");
        if (trimmed.len == 0) {
            if (self.current_data) |data| {
                const event_type = self.current_event orelse "message";
                renderSseEvent(self.alloc, event_type, data);
                if (std.mem.eql(u8, event_type, "run_complete")) self.done = true;
            }
            if (self.current_data) |d| self.alloc.free(d);
            self.current_data = null;
            if (self.current_event) |e| self.alloc.free(e);
            self.current_event = null;
            return;
        }
        if (std.mem.startsWith(u8, trimmed, "data: ")) {
            if (self.current_data) |d| self.alloc.free(d);
            self.current_data = self.alloc.dupe(u8, trimmed["data: ".len..]) catch null;
        } else if (std.mem.startsWith(u8, trimmed, "event: ")) {
            if (self.current_event) |e| self.alloc.free(e);
            self.current_event = self.alloc.dupe(u8, trimmed["event: ".len..]) catch null;
        }
    }

    pub fn feedBytes(self: *SseState, bytes: []const u8) void {
        for (bytes) |b| {
            if (b == '\n') {
                self.processLine(self.line_buf.items);
                self.line_buf.clearRetainingCapacity();
            } else {
                self.line_buf.append(self.alloc, b) catch {};
            }
        }
    }
};

pub fn streamRunOutput(
    alloc: std.mem.Allocator,
    base_url: []const u8,
    api_key: []const u8,
    run_id: []const u8,
) !void {
    const url = try std.fmt.allocPrint(alloc, "{s}/v1/runs/{s}:stream", .{ base_url, run_id });
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{api_key});
    defer alloc.free(auth_header);

    var client = std.http.Client{ .allocator = alloc };
    defer client.deinit();

    const uri = try std.Uri.parse(url);

    // §1: Use client.open() for true incremental streaming.
    // client.fetch() buffers the entire response before returning — defeats SSE real-time.
    // client.open() + req.reader().read() delivers bytes as they arrive from the server.
    var header_buf: [4096]u8 = undefined;
    var req = try client.open(.GET, uri, .{
        .extra_headers = &.{
            .{ .name = "Authorization", .value = auth_header },
            .{ .name = "Accept", .value = "text/event-stream" },
        },
        .server_header_buffer = &header_buf,
    });
    defer req.deinit();

    try req.send();
    try req.wait();

    if (req.status != .ok) {
        std.debug.print("error: stream returned {d}\n", .{@intFromEnum(req.status)});
        return error.StreamFailed;
    }

    var sse = SseState.init(alloc);
    defer sse.deinit();
    var buf: [4096]u8 = undefined;
    while (true) {
        const n = req.reader().read(&buf) catch |err| {
            std.debug.print("error: stream read failed: {s}\n", .{@errorName(err)});
            break;
        };
        if (n == 0) break;
        sse.feedBytes(buf[0..n]);
        if (sse.done) break;
    }
}

pub fn renderSseEvent(alloc: std.mem.Allocator, event_type: []const u8, data: []const u8) void {
    if (std.mem.eql(u8, event_type, "run_complete")) {
        std.debug.print("[done] run complete: {s}\n", .{data});
        return;
    }
    if (!std.mem.eql(u8, event_type, "gate_result")) return;

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, data, .{}) catch return;
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return,
    };

    const gate_name = if (obj.get("gate_name")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";
    const outcome = if (obj.get("outcome")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";
    const loop_n = if (obj.get("loop")) |v| switch (v) {
        .integer => |i| i,
        else => 0,
    } else 0;
    const wall_ms = if (obj.get("wall_ms")) |v| switch (v) {
        .integer => |i| i,
        else => 0,
    } else 0;

    std.debug.print("[{s}] {s} (loop {d}, {d}ms)\n", .{ gate_name, outcome, loop_n, wall_ms });
}
