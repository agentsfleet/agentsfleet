//! Unix socket server and client for the executor sidecar.
//!
//! The transport layer manages Unix domain socket connections and dispatches
//! incoming frames to a handler callback. Worker-to-executor is 1:1 in v1.

const std = @import("std");
const builtin = @import("builtin");
const protocol = @import("protocol.zig");

const log = std.log.scoped(.executor_transport);

pub const ConnectionError = error{
    SocketBindFailed,
    SocketConnectFailed,
    ConnectionClosed,
    FrameTooLarge,
};

/// Handler callback: receives raw frame payload, returns response payload.
/// Caller owns both the input and output slices.
pub const FrameHandler = *const fn (alloc: std.mem.Allocator, payload: []const u8) anyerror![]u8;

/// Unix socket server for the executor sidecar.
pub const Server = struct {
    socket_path: []const u8,
    listener: ?std.posix.socket_t = null,
    running: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    handler: FrameHandler,
    alloc: std.mem.Allocator,

    pub fn init(alloc: std.mem.Allocator, socket_path: []const u8, handler: FrameHandler) Server {
        return .{
            .socket_path = socket_path,
            .handler = handler,
            .alloc = alloc,
        };
    }

    pub fn bind(self: *Server) !void {
        std.fs.deleteFileAbsolute(self.socket_path) catch {};

        const sock = try std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0);
        errdefer std.posix.close(sock);

        var addr: std.posix.sockaddr.un = std.mem.zeroes(std.posix.sockaddr.un);
        addr.family = std.posix.AF.UNIX;
        const path_len = @min(self.socket_path.len, addr.path.len - 1);
        @memcpy(addr.path[0..path_len], self.socket_path[0..path_len]);

        std.posix.bind(sock, @ptrCast(&addr), @sizeOf(std.posix.sockaddr.un)) catch {
            return ConnectionError.SocketBindFailed;
        };
        std.posix.listen(sock, 1) catch {
            return ConnectionError.SocketBindFailed;
        };

        self.listener = sock;
        self.running.store(true, .release);
        log.info("executor.server_bound path={s}", .{self.socket_path});
    }

    pub fn serve(self: *Server) void {
        const listener = self.listener orelse return;

        while (self.running.load(.acquire)) {
            // Poll with 200ms timeout so stop() can unblock us by setting running=false.
            // A blocking accept() on macOS does not reliably wake when the listener fd
            // is closed from another thread.
            var fds = [1]std.posix.pollfd{.{
                .fd = listener,
                .events = std.posix.POLL.IN,
                .revents = 0,
            }};
            const ready = std.posix.poll(&fds, 200) catch continue;
            if (ready == 0) continue; // timeout — re-check running flag
            // Re-check running after poll returns: stop() may have closed the
            // listener fd while poll was blocked. Calling accept() on a closed
            // fd triggers EBADF which is unreachable in std.posix — guard here.
            if (!self.running.load(.acquire)) break;

            const conn = std.posix.accept(listener, null, null, 0) catch |err| {
                if (!self.running.load(.acquire)) break;
                log.warn("executor.accept_failed err={s}", .{@errorName(err)});
                continue;
            };

            self.handleConnection(conn);
            std.posix.close(conn);
        }
    }

    fn handleConnection(self: *Server, conn: std.posix.socket_t) void {
        while (self.running.load(.acquire)) {
            // Poll the connection fd so we can check running periodically.
            // Without this, readFrameFromFd blocks forever if stop() is
            // called while the client connection is still open.
            var fds = [1]std.posix.pollfd{.{
                .fd = conn,
                .events = std.posix.POLL.IN,
                .revents = 0,
            }};
            const ready = std.posix.poll(&fds, 200) catch break;
            if (ready == 0) continue;

            const frame = protocol.readFrameFromFd(self.alloc, conn) catch |err| {
                switch (err) {
                    error.ConnectionClosed => {},
                    else => log.warn("executor.read_failed err={s}", .{@errorName(err)}),
                }
                break;
            };
            defer self.alloc.free(frame);

            const response = self.handler(self.alloc, frame) catch |err| {
                log.err("executor.handler_failed err={s}", .{@errorName(err)});
                break;
            };
            defer self.alloc.free(response);

            protocol.writeFrameToFd(conn, response) catch |err| {
                log.warn("executor.write_failed err={s}", .{@errorName(err)});
                break;
            };
        }
    }

    pub fn stop(self: *Server) void {
        self.running.store(false, .release);
        if (self.listener) |sock| {
            std.posix.close(sock);
            self.listener = null;
        }
        std.fs.deleteFileAbsolute(self.socket_path) catch {};
    }
};

/// Unix socket client for the worker side.
pub const Client = struct {
    socket_path: []const u8,
    fd: ?std.posix.socket_t = null,
    alloc: std.mem.Allocator,

    pub fn init(alloc: std.mem.Allocator, socket_path: []const u8) Client {
        return .{
            .socket_path = socket_path,
            .alloc = alloc,
        };
    }

    pub fn connect(self: *Client) !void {
        const sock = try std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0);
        errdefer std.posix.close(sock);

        var addr: std.posix.sockaddr.un = std.mem.zeroes(std.posix.sockaddr.un);
        addr.family = std.posix.AF.UNIX;
        const path_len = @min(self.socket_path.len, addr.path.len - 1);
        @memcpy(addr.path[0..path_len], self.socket_path[0..path_len]);

        std.posix.connect(sock, @ptrCast(&addr), @sizeOf(std.posix.sockaddr.un)) catch {
            return ConnectionError.SocketConnectFailed;
        };

        self.fd = sock;
    }

    pub fn sendRequest(self: *Client, id: u64, method: []const u8, params: ?std.json.Value) !protocol.ParsedResponse {
        const sock = self.fd orelse return error.ConnectionClosed;
        try protocol.sendRequest(self.alloc, sock, id, method, params);
        const frame = try protocol.readFrameFromFd(self.alloc, sock);
        defer self.alloc.free(frame);
        return protocol.parseResponse(self.alloc, frame);
    }

    pub fn close(self: *Client) void {
        if (self.fd) |sock| {
            std.posix.close(sock);
            self.fd = null;
        }
    }
};

var test_socket_counter = std.atomic.Value(u32).init(0);

fn makeTestSocketPath(alloc: std.mem.Allocator, label: []const u8) ![]u8 {
    const suffix = test_socket_counter.fetchAdd(1, .acq_rel);
    const stamp: u64 = @intCast(@max(std.time.nanoTimestamp(), 0));
    return std.fmt.allocPrint(alloc, "/tmp/zombie-{s}-{d}-{d}.sock", .{ label, stamp, suffix });
}

test "Server and Client communicate over Unix socket" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "executor-test");
    defer alloc.free(path);
    const connect_retry_sleep_ms = 20;
    const connect_retry_count = 50;

    const echo_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    var server = Server.init(alloc, path, echo_handler);
    try server.bind();
    defer server.stop();

    const server_thread = try std.Thread.spawn(.{}, Server.serve, .{&server});
    defer server_thread.join();

    var client = Client.init(alloc, path);
    var connected = false;
    var retry_index: usize = 0;
    while (retry_index < connect_retry_count) : (retry_index += 1) {
        client.connect() catch |err| switch (err) {
            ConnectionError.SocketConnectFailed => {
                std.Thread.sleep(connect_retry_sleep_ms * std.time.ns_per_ms);
                continue;
            },
            else => return err,
        };
        connected = true;
        break;
    }
    if (!connected) return error.SocketConnectFailed;
    defer client.close();

    const sock = client.fd.?;
    const payload = "test frame data";
    try protocol.writeFrameToFd(sock, payload);
    const response = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(response);

    try std.testing.expectEqualStrings(payload, response);

    client.close();
    server.stop();
}

test "Client.sendRequest rejects use before connect" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const path = try makeTestSocketPath(std.testing.allocator, "executor-unconnected");
    defer std.testing.allocator.free(path);

    var client = Client.init(std.testing.allocator, path);
    try std.testing.expectError(error.ConnectionClosed, client.sendRequest(1, "ping", null));
}

test "Server.stop unblocks idle serve loop without leaked listener" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const echo_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    const path = try makeTestSocketPath(std.testing.allocator, "executor-stop-test");
    defer std.testing.allocator.free(path);

    var server = Server.init(std.testing.allocator, path, echo_handler);
    try server.bind();
    const thread = try std.Thread.spawn(.{}, Server.serve, .{&server});
    std.Thread.sleep(30 * std.time.ns_per_ms);
    server.stop();
    thread.join();
    try std.testing.expect(server.listener == null);
}

// ── Unit tests for transport structs, constants, and pure initialization ──

test "Server.init returns correct default state" {
    const noop_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    const server = Server.init(std.testing.allocator, "/tmp/test.sock", noop_handler);
    try std.testing.expectEqualStrings("/tmp/test.sock", server.socket_path);
    try std.testing.expect(server.listener == null);
    try std.testing.expect(server.running.load(.acquire) == false);
    try std.testing.expect(server.handler == noop_handler);
}

test "Client.init returns correct default state" {
    const client = Client.init(std.testing.allocator, "/tmp/client-test.sock");
    try std.testing.expectEqualStrings("/tmp/client-test.sock", client.socket_path);
    try std.testing.expect(client.fd == null);
}

test "Client.close on unconnected client is a no-op" {
    var client = Client.init(std.testing.allocator, "/tmp/noop.sock");
    // Must not panic or error — just a no-op.
    client.close();
    try std.testing.expect(client.fd == null);
}

test "Server.stop on unbound server is a no-op" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const noop_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    var server = Server.init(std.testing.allocator, "/tmp/never-bound.sock", noop_handler);
    // Must not panic — listener is null, running is false.
    server.stop();
    try std.testing.expect(server.listener == null);
    try std.testing.expect(server.running.load(.acquire) == false);
}

test "Server.serve returns immediately when listener is null" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const noop_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    var server = Server.init(std.testing.allocator, "/tmp/no-listener.sock", noop_handler);
    // serve() with null listener should return immediately, not block.
    server.serve();
    try std.testing.expect(server.listener == null);
}

test "ConnectionError variants are distinct" {
    // Compile-time assertion: all four error variants exist and are usable.
    const err1: ConnectionError = ConnectionError.SocketBindFailed;
    const err2: ConnectionError = ConnectionError.SocketConnectFailed;
    const err3: ConnectionError = ConnectionError.ConnectionClosed;
    const err4: ConnectionError = ConnectionError.FrameTooLarge;
    try std.testing.expect(err1 != err2);
    try std.testing.expect(err2 != err3);
    try std.testing.expect(err3 != err4);
}

test "ConnectionError names are stable" {
    try std.testing.expectEqualStrings("SocketBindFailed", @errorName(ConnectionError.SocketBindFailed));
    try std.testing.expectEqualStrings("SocketConnectFailed", @errorName(ConnectionError.SocketConnectFailed));
    try std.testing.expectEqualStrings("ConnectionClosed", @errorName(ConnectionError.ConnectionClosed));
    try std.testing.expectEqualStrings("FrameTooLarge", @errorName(ConnectionError.FrameTooLarge));
}

test "makeTestSocketPath produces valid format" {
    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "fmt-test");
    defer alloc.free(path);

    // Must start with /tmp/zombie- prefix.
    try std.testing.expect(std.mem.startsWith(u8, path, "/tmp/zombie-fmt-test-"));
    // Must end with .sock extension.
    try std.testing.expect(std.mem.endsWith(u8, path, ".sock"));
    // Must be a reasonable length for a Unix socket path (< 104 on most systems).
    try std.testing.expect(path.len < 104);
}

test "makeTestSocketPath generates unique paths" {
    const alloc = std.testing.allocator;
    const path1 = try makeTestSocketPath(alloc, "unique");
    defer alloc.free(path1);
    const path2 = try makeTestSocketPath(alloc, "unique");
    defer alloc.free(path2);

    // Atomic counter guarantees different suffixes even with same label.
    try std.testing.expect(!std.mem.eql(u8, path1, path2));
}

test "Server struct size is non-zero and reasonable" {
    // Sanity: Server should contain a few fields — not empty, not bloated.
    const size = @sizeOf(Server);
    try std.testing.expect(size > 0);
    try std.testing.expect(size < 1024);
}

test "Client struct size is non-zero and reasonable" {
    const size = @sizeOf(Client);
    try std.testing.expect(size > 0);
    try std.testing.expect(size < 512);
}

test "FrameHandler type is a function pointer" {
    // Compile-time check: FrameHandler is callable with the expected signature.
    const handler_info = @typeInfo(FrameHandler);
    try std.testing.expect(handler_info == .pointer);
}
