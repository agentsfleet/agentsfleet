// In-process SSE client for HTTP integration tests.
//
// Opens a plain TCP connection to the in-process TestHarness server, sends
// an HTTP/1.1 GET with a Bearer header, parses out the status line + headers,
// then yields parsed SSE frames {id, event, data} from the body. Supports
// unilateral mid-stream disconnect for the reconnect-test choreography.
//
// SO_RCVTIMEO drives the read deadline so a missing frame surfaces as
// `error.SseFrameTimeout` instead of hanging the test.
//
// File-as-struct: this file IS the `SseClient`.

pub const SseClient = @This();

alloc: Allocator,
stream: std.net.Stream,
buf: std.ArrayList(u8),
closed: bool = false,

pub const ConnectOptions = struct {
    bearer: []const u8,
    /// SO_RCVTIMEO applied for both header parse + frame reads.
    deadline_ms: u32 = 4_000,
    /// Optional `Last-Event-ID` request header. The handler ignores it by
    /// design — `test_sse_sequence_resets_on_reconnect` uses this to assert
    /// the server still emits id=0 on reconnect.
    last_event_id: ?[]const u8 = null,
};

pub const Frame = struct {
    id: []u8,
    event: []u8,
    data: []u8,

    pub fn deinit(self: *Frame, alloc: Allocator) void {
        alloc.free(self.id);
        alloc.free(self.event);
        alloc.free(self.data);
    }
};

pub fn connect(alloc: Allocator, port: u16, path: []const u8, opts: ConnectOptions) !SseClient {
    const addr = try std.net.Address.parseIp4("127.0.0.1", port);
    const stream = try std.net.tcpConnectToAddress(addr);
    var sc = SseClient{
        .alloc = alloc,
        .stream = stream,
        .buf = std.ArrayList(u8){},
    };
    errdefer sc.deinit();
    setReadTimeout(stream.handle, opts.deadline_ms);
    try sc.sendRequest(port, path, opts);
    try sc.consumeResponseHeaders();
    return sc;
}

pub fn deinit(self: *SseClient) void {
    if (!self.closed) {
        self.stream.close();
        self.closed = true;
    }
    self.buf.deinit(self.alloc);
}

/// Unilateral mid-stream disconnect — close the TCP socket without sending
/// a graceful HTTP close. Sets SO_LINGER=0 so close() sends RST instead of
/// FIN; the server's next write fails with EPIPE on the first attempt
/// rather than buffering silently behind a half-closed FIN. That release
/// lets the SSE handler's worker thread exit during test teardown even
/// though the production-side subscriber loop has no read timeout.
pub fn closeStream(self: *SseClient) void {
    if (self.closed) return;
    forceRstOnClose(self.stream.handle);
    self.stream.close();
    self.closed = true;
}

/// Block until the next SSE frame arrives, the socket times out (returns
/// `error.SseFrameTimeout`), or the server closes (`error.EndOfStream`).
/// Caller owns the returned frame's slices and must `deinit` them.
pub fn nextFrame(self: *SseClient) !Frame {
    while (true) {
        if (findFrameDelimiter(self.buf.items)) |end| {
            const frame_bytes = self.buf.items[0..end];
            const frame = try parseFrame(self.alloc, frame_bytes);
            self.consume(end + 2); // skip the trailing "\n\n"
            return frame;
        }
        try self.fill();
    }
}

fn sendRequest(self: *SseClient, port: u16, path: []const u8, opts: ConnectOptions) !void {
    var req_buf: [2048]u8 = undefined;
    const req = if (opts.last_event_id) |leid| try std.fmt.bufPrint(&req_buf, "GET {s} HTTP/1.1\r\nHost: 127.0.0.1:{d}\r\nAuthorization: Bearer {s}\r\nAccept: text/event-stream\r\nLast-Event-ID: {s}\r\nConnection: keep-alive\r\n\r\n", .{ path, port, opts.bearer, leid }) else try std.fmt.bufPrint(&req_buf, "GET {s} HTTP/1.1\r\nHost: 127.0.0.1:{d}\r\nAuthorization: Bearer {s}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n", .{ path, port, opts.bearer });
    try self.stream.writeAll(req);
}

fn consumeResponseHeaders(self: *SseClient) !void {
    while (std.mem.indexOf(u8, self.buf.items, "\r\n\r\n") == null) try self.fill();

    const eoh = std.mem.indexOf(u8, self.buf.items, "\r\n\r\n").?;
    const headers = self.buf.items[0..eoh];
    const status_line_end = std.mem.indexOf(u8, headers, "\r\n") orelse return error.SseInvalidStatusLine;
    const status_line = headers[0..status_line_end];
    if (status_line.len < 12 or !std.mem.startsWith(u8, status_line, "HTTP/1.1 ")) return error.SseInvalidStatusLine;

    const status = std.fmt.parseInt(u16, status_line[9..12], 10) catch return error.SseInvalidStatusLine;
    if (status != 200) return error.SseUnexpectedStatus;

    self.consume(eoh + 4);
}

fn fill(self: *SseClient) !void {
    var tmp: [4096]u8 = undefined;
    const n = self.stream.read(&tmp) catch |err| switch (err) {
        error.WouldBlock => return error.SseFrameTimeout,
        else => return err,
    };
    if (n == 0) return error.EndOfStream;
    try self.buf.appendSlice(self.alloc, tmp[0..n]);
}

fn consume(self: *SseClient, n: usize) void {
    if (n == 0) return;
    if (n >= self.buf.items.len) {
        self.buf.clearRetainingCapacity();
        return;
    }
    std.mem.copyForwards(u8, self.buf.items[0 .. self.buf.items.len - n], self.buf.items[n..]);
    self.buf.shrinkRetainingCapacity(self.buf.items.len - n);
}

fn findFrameDelimiter(bytes: []const u8) ?usize {
    return std.mem.indexOf(u8, bytes, "\n\n");
}

fn parseFrame(alloc: Allocator, frame_bytes: []const u8) !Frame {
    var id_val: []const u8 = "";
    var event_val: []const u8 = "";
    var data_val: []const u8 = "";

    var lines = std.mem.splitScalar(u8, frame_bytes, '\n');
    while (lines.next()) |raw| {
        const line = if (std.mem.endsWith(u8, raw, "\r")) raw[0 .. raw.len - 1] else raw;
        if (line.len == 0) continue;
        const colon = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const name = line[0..colon];
        var value = line[colon + 1 ..];
        if (value.len > 0 and value[0] == ' ') value = value[1..];
        if (std.mem.eql(u8, name, "id")) {
            id_val = value;
        } else if (std.mem.eql(u8, name, "event")) {
            event_val = value;
        } else if (std.mem.eql(u8, name, "data")) {
            data_val = value;
        }
    }

    return .{
        .id = try alloc.dupe(u8, id_val),
        .event = try alloc.dupe(u8, event_val),
        .data = try alloc.dupe(u8, data_val),
    };
}

fn setReadTimeout(fd: std.posix.fd_t, ms: u32) void {
    const timeout = std.posix.timeval{
        .sec = @intCast(ms / 1000),
        .usec = @intCast((ms % 1000) * 1000),
    };
    std.posix.setsockopt(fd, std.posix.SOL.SOCKET, std.posix.SO.RCVTIMEO, std.mem.asBytes(&timeout)) catch {};
}

const Linger = extern struct { onoff: c_int, linger: c_int };

fn forceRstOnClose(fd: std.posix.fd_t) void {
    const opt = Linger{ .onoff = 1, .linger = 0 };
    std.posix.setsockopt(fd, std.posix.SOL.SOCKET, std.posix.SO.LINGER, std.mem.asBytes(&opt)) catch {};
}

const std = @import("std");
const Allocator = std.mem.Allocator;

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "parseFrame: id + event + data with leading space stripped" {
    const bytes = "id: 7\nevent: chunk\ndata: {\"text\":\"hi\"}";
    var f = try parseFrame(testing.allocator, bytes);
    defer f.deinit(testing.allocator);
    try testing.expectEqualStrings("7", f.id);
    try testing.expectEqualStrings("chunk", f.event);
    try testing.expectEqualStrings("{\"text\":\"hi\"}", f.data);
}

test "parseFrame: tolerates CRLF line endings" {
    const bytes = "id: 0\r\nevent: event_received\r\ndata: {}\r";
    var f = try parseFrame(testing.allocator, bytes);
    defer f.deinit(testing.allocator);
    try testing.expectEqualStrings("0", f.id);
    try testing.expectEqualStrings("event_received", f.event);
    try testing.expectEqualStrings("{}", f.data);
}

test "parseFrame: missing fields return empty strings, not error" {
    const bytes = "data: just-data";
    var f = try parseFrame(testing.allocator, bytes);
    defer f.deinit(testing.allocator);
    try testing.expectEqualStrings("", f.id);
    try testing.expectEqualStrings("", f.event);
    try testing.expectEqualStrings("just-data", f.data);
}

test "findFrameDelimiter: locates blank line between frames" {
    const buf = "id: 1\nevent: a\ndata: x\n\nid: 2\nevent: b\ndata: y\n\n";
    const idx = findFrameDelimiter(buf) orelse return error.TestFailed;
    try testing.expectEqualStrings("id: 1\nevent: a\ndata: x", buf[0..idx]);
}
