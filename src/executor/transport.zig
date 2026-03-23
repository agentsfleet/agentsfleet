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

test "Server and Client communicate over Unix socket" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    const path = "/tmp/zombie-executor-test.sock";

    const echo_handler = struct {
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return a.dupe(u8, payload);
        }
    }.handle;

    var server = Server.init(alloc, path, echo_handler);
    try server.bind();
    defer server.stop();

    const server_thread = try std.Thread.spawn(.{}, Server.serve, .{&server});

    std.Thread.sleep(50 * std.time.ns_per_ms);

    var client = Client.init(alloc, path);
    try client.connect();
    defer client.close();

    const sock = client.fd.?;
    const payload = "test frame data";
    try protocol.writeFrameToFd(sock, payload);
    const response = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(response);

    try std.testing.expectEqualStrings(payload, response);

    client.close();
    server.stop();
    server_thread.join();
}
