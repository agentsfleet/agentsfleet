//! Unix socket server and client for the executor sidecar.
//!
//! The transport layer manages Unix domain socket connections and dispatches
//! incoming frames to a handler callback. Worker-to-executor is 1:1 in v1.

const std = @import("std");
const builtin = @import("builtin");
const protocol = @import("protocol.zig");
const pc = @import("progress_callbacks.zig");

const log = std.log.scoped(.executor_transport);

pub const ConnectionError = error{
    SocketBindFailed,
    SocketConnectFailed,
    ConnectionClosed,
    FrameTooLarge,
    /// Peer advertised a different rpc_version. Pre-v2.0.0 there is no
    /// v1 compat shim; both sides log + abort the connection.
    RpcVersionMismatch,
};

/// Exchange HELLO frames symmetrically. Both peers write their own
/// HELLO first (kernel-buffered on Unix sockets) then read the
/// counterparty's HELLO — no deadlock, no order dependency.
///
/// Mismatch on `rpc_version` returns `RpcVersionMismatch`; caller
/// closes the fd. Logs the failure for both peers (worker side gets
/// it from the returned error; executor side logs in
/// `Server.handleConnection`).
fn exchangeHello(alloc: std.mem.Allocator, fd: std.posix.socket_t) !void {
    const ours = try pc.encodeHello(alloc, .{ .rpc_version = pc.rpc_version });
    defer alloc.free(ours);
    try protocol.writeFrameToFd(fd, ours);

    const theirs = try protocol.readFrameFromFd(alloc, fd);
    defer alloc.free(theirs);

    const peer = try pc.decodeHello(alloc, theirs);
    if (peer.rpc_version != pc.rpc_version) return ConnectionError.RpcVersionMismatch;
}

/// Handler callback: receives raw frame payload, returns response payload.
/// Caller owns both the input and output slices.
const FrameHandler = *const fn (alloc: std.mem.Allocator, payload: []const u8) anyerror![]u8;

/// Context + thunk closure for receiving progress frames from
/// `Client.sendRequestStreaming`. Zig has no first-class closures;
/// callers build one of these manually with a pointer to their own
/// state plus a function that knows how to decode that pointer.
///
/// Worker usage (slice 4): `ctx` points at a `(redis_client*,
/// zombie_id, event_id)` bundle; `emit_fn` PUBLISHes the frame onto
/// `zombie:{id}:activity` via the activity_publisher helpers.
pub const ProgressEmitter = struct {
    ctx: *anyopaque,
    emit_fn: *const fn (ctx: *anyopaque, frame: pc.ProgressFrame) void,

    pub fn emit(self: ProgressEmitter, frame: pc.ProgressFrame) void {
        self.emit_fn(self.ctx, frame);
    }

    /// No-op emitter for callers that have no live-tail consumer
    /// (e.g. unit tests of the transport layer, or the
    /// non-progress-aware paths).
    pub fn noop() ProgressEmitter {
        return .{ .ctx = undefined, .emit_fn = noopEmit };
    }
};

fn noopEmit(_: *anyopaque, _: pc.ProgressFrame) void {}

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
        exchangeHello(self.alloc, conn) catch |err| {
            log.err("executor.rpc_version_mismatch peer=client err={s}", .{@errorName(err)});
            return;
        };

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

        exchangeHello(self.alloc, sock) catch |err| {
            log.err("executor.rpc_version_mismatch peer=server err={s}", .{@errorName(err)});
            return err;
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

    /// Send one request, then read frames in a loop until the terminal
    /// response arrives. Each progress frame is dispatched to `emitter`
    /// before the next read; the terminal response is returned to the
    /// caller as `ParsedResponse`.
    ///
    /// Today (slice 3b) the executor never emits progress frames, so
    /// the loop runs exactly once and degenerates to the same wire
    /// behaviour as `sendRequest`. When slice 3c wires NullClaw
    /// lifecycle hooks into the executor-side emitter, this loop is
    /// what surfaces them to the worker without any caller change.
    pub fn sendRequestStreaming(
        self: *Client,
        id: u64,
        method: []const u8,
        params: ?std.json.Value,
        emitter: ProgressEmitter,
    ) !protocol.ParsedResponse {
        const sock = self.fd orelse return error.ConnectionClosed;
        try protocol.sendRequest(self.alloc, sock, id, method, params);

        while (true) {
            const frame = try protocol.readFrameFromFd(self.alloc, sock);
            // Parse once to discriminate progress vs terminal. Progress
            // frames carry method="Progress"; terminal frames carry
            // result/error and no method.
            var parsed = std.json.parseFromSlice(std.json.Value, self.alloc, frame, .{}) catch |err| {
                self.alloc.free(frame);
                return err;
            };
            const is_progress = pc.isProgressPayload(parsed.value);
            parsed.deinit();

            if (is_progress) {
                defer self.alloc.free(frame);
                var pres = pc.decodeProgress(self.alloc, frame) catch |err| {
                    log.warn("executor.progress_decode_failed err={s}", .{@errorName(err)});
                    continue;
                };
                defer pres.parsed.deinit();
                emitter.emit(pres.frame);
                continue;
            }

            // Terminal response. Caller owns the returned ParsedResponse;
            // its internal `parsed` holds the frame bytes via the JSON
            // value, so we hand off the frame buffer ownership too.
            defer self.alloc.free(frame);
            return try protocol.parseResponse(self.alloc, frame);
        }
    }

    pub fn close(self: *Client) void {
        if (self.fd) |sock| {
            std.posix.close(sock);
            self.fd = null;
        }
    }
};

// Tests live in transport_test.zig (sibling _test.zig — pattern matches
// client_test.zig / runner_test.zig). The split keeps this file under
// the 350-line cap after the M42 RPC v2 additions.
