const std = @import("std");
const builtin = @import("builtin");
const protocol = @import("protocol.zig");
const pc = @import("progress_callbacks.zig");
const transport = @import("transport.zig");
const Server = transport.Server;
const Client = transport.Client;
const ConnectionError = transport.ConnectionError;
const ProgressEmitter = transport.ProgressEmitter;

var test_socket_counter = std.atomic.Value(u32).init(0);

fn makeTestSocketPath(alloc: std.mem.Allocator, label: []const u8) ![]u8 {
    const suffix = test_socket_counter.fetchAdd(1, .acq_rel);
    const stamp: u64 = @intCast(@max(std.time.nanoTimestamp(), 0));
    return std.fmt.allocPrint(alloc, "/tmp/zombie-{s}-{d}-{d}.sock", .{ label, stamp, suffix });
}

const echo_handler = struct {
    fn handle(a: std.mem.Allocator, payload: []const u8, _: ?std.posix.socket_t) anyerror![]u8 {
        return a.dupe(u8, payload);
    }
}.handle;

fn connectWithRetry(client: *Client) !void {
    const retry_sleep_ms = 20;
    const retry_count = 50;
    var i: usize = 0;
    while (i < retry_count) : (i += 1) {
        client.connect() catch |err| switch (err) {
            ConnectionError.SocketConnectFailed => {
                std.Thread.sleep(retry_sleep_ms * std.time.ns_per_ms);
                continue;
            },
            else => return err,
        };
        return;
    }
    return ConnectionError.SocketConnectFailed;
}

test "Server and Client communicate over Unix socket" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "executor-test");
    defer alloc.free(path);

    var server = Server.init(alloc, path, echo_handler);
    try server.bind();
    defer server.stop();
    const server_thread = try std.Thread.spawn(.{}, Server.serve, .{&server});
    defer server_thread.join();

    var client = Client.init(alloc, path);
    try connectWithRetry(&client);
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

test "Server.init returns correct default state" {
    const server = Server.init(std.testing.allocator, "/tmp/test.sock", echo_handler);
    try std.testing.expectEqualStrings("/tmp/test.sock", server.socket_path);
    try std.testing.expect(server.listener == null);
    try std.testing.expect(server.running.load(.acquire) == false);
}

test "Client.init returns correct default state" {
    const client = Client.init(std.testing.allocator, "/tmp/client-test.sock");
    try std.testing.expectEqualStrings("/tmp/client-test.sock", client.socket_path);
    try std.testing.expect(client.fd == null);
}

test "Client.close on unconnected client is a no-op" {
    var client = Client.init(std.testing.allocator, "/tmp/noop.sock");
    client.close();
    try std.testing.expect(client.fd == null);
}

test "Server.stop on unbound server is a no-op" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    var server = Server.init(std.testing.allocator, "/tmp/never-bound.sock", echo_handler);
    server.stop();
    try std.testing.expect(server.listener == null);
}

test "Server.serve returns immediately when listener is null" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    var server = Server.init(std.testing.allocator, "/tmp/no-listener.sock", echo_handler);
    server.serve();
    try std.testing.expect(server.listener == null);
}

test "ConnectionError variants are distinct and named" {
    try std.testing.expectEqualStrings("SocketBindFailed", @errorName(ConnectionError.SocketBindFailed));
    try std.testing.expectEqualStrings("SocketConnectFailed", @errorName(ConnectionError.SocketConnectFailed));
    try std.testing.expectEqualStrings("ConnectionClosed", @errorName(ConnectionError.ConnectionClosed));
    try std.testing.expectEqualStrings("FrameTooLarge", @errorName(ConnectionError.FrameTooLarge));
    try std.testing.expectEqualStrings("RpcVersionMismatch", @errorName(ConnectionError.RpcVersionMismatch));
}

// ── HELLO handshake (M42 RPC v2) ──────────────────────────────────────

test "HELLO handshake: matched versions complete connect cleanly" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "exec-hello-match");
    defer alloc.free(path);

    var server = Server.init(alloc, path, echo_handler);
    try server.bind();
    defer server.stop();
    const server_thread = try std.Thread.spawn(.{}, Server.serve, .{&server});
    defer server_thread.join();

    var client = Client.init(alloc, path);
    try connectWithRetry(&client);
    defer client.close();

    // After the handshake, both peers can exchange normal frames.
    const sock = client.fd.?;
    try protocol.writeFrameToFd(sock, "post-hello");
    const response = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(response);
    try std.testing.expectEqualStrings("post-hello", response);

    client.close();
    server.stop();
}

test "HELLO handshake: client connecting to a fake-v1 server fails fast" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "exec-hello-bad-server");
    defer alloc.free(path);

    // Hand-rolled server that sends a HELLO with rpc_version=1 (impersonates
    // a pre-M42 executor). Client's handshake should detect the mismatch
    // and fail fast with RpcVersionMismatch.
    std.fs.deleteFileAbsolute(path) catch {};
    const listener = try std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0);
    defer std.posix.close(listener);
    var addr: std.posix.sockaddr.un = std.mem.zeroes(std.posix.sockaddr.un);
    addr.family = std.posix.AF.UNIX;
    const path_len = @min(path.len, addr.path.len - 1);
    @memcpy(addr.path[0..path_len], path[0..path_len]);
    try std.posix.bind(listener, @ptrCast(&addr), @sizeOf(std.posix.sockaddr.un));
    try std.posix.listen(listener, 1);
    defer std.fs.deleteFileAbsolute(path) catch {};

    const FakeServer = struct {
        fn run(l: std.posix.socket_t, a: std.mem.Allocator) void {
            const conn = std.posix.accept(l, null, null, 0) catch return;
            defer std.posix.close(conn);
            const bad_hello = pc.encodeHello(a, .{ .rpc_version = 1 }) catch return;
            defer a.free(bad_hello);
            protocol.writeFrameToFd(conn, bad_hello) catch return;
            // Drain whatever the client sent (it sends its own HELLO before reading ours).
            const drained = protocol.readFrameFromFd(a, conn) catch return;
            a.free(drained);
        }
    };
    const t = try std.Thread.spawn(.{}, FakeServer.run, .{ listener, alloc });
    defer t.join();

    var client = Client.init(alloc, path);
    try std.testing.expectError(ConnectionError.RpcVersionMismatch, connectWithRetry(&client));
}

// ── sendRequestStreaming: streaming-reader fallback when no progress arrives ──

test "sendRequestStreaming returns terminal cleanly when zero progress frames arrive" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "exec-streaming-degenerate");
    defer alloc.free(path);

    // Echo handler crafts a JSON-RPC-shaped terminal response so
    // parseResponse works; no progress frames are ever emitted.
    const rpc_terminal_handler = struct {
        fn handle(a: std.mem.Allocator, _: []const u8, _: ?std.posix.socket_t) anyerror![]u8 {
            return std.json.Stringify.valueAlloc(a, .{
                .jsonrpc = "2.0",
                .id = 7,
                .result = .{ .ok = true },
            }, .{});
        }
    }.handle;

    var server = Server.init(alloc, path, rpc_terminal_handler);
    try server.bind();
    defer server.stop();
    const server_thread = try std.Thread.spawn(.{}, Server.serve, .{&server});
    defer server_thread.join();

    var client = Client.init(alloc, path);
    try connectWithRetry(&client);
    defer client.close();

    const emitter = ProgressEmitter.noop();
    var resp = try client.sendRequestStreaming(7, "TestMethod", null, emitter);
    defer resp.deinit();

    try std.testing.expectEqual(@as(u64, 7), resp.id);
    try std.testing.expect(resp.result != null);
    try std.testing.expect(resp.rpc_error == null);

    client.close();
    server.stop();
}

test "sendRequestStreaming dispatches progress frames before returning terminal" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const path = try makeTestSocketPath(alloc, "exec-streaming-with-progress");
    defer alloc.free(path);

    // Hand-rolled server that, on receiving a request, writes 3 progress
    // frames + 1 terminal — exercising the multiplexed reply path.
    std.fs.deleteFileAbsolute(path) catch {};
    const listener = try std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0);
    defer std.posix.close(listener);
    var addr: std.posix.sockaddr.un = std.mem.zeroes(std.posix.sockaddr.un);
    addr.family = std.posix.AF.UNIX;
    const path_len = @min(path.len, addr.path.len - 1);
    @memcpy(addr.path[0..path_len], path[0..path_len]);
    try std.posix.bind(listener, @ptrCast(&addr), @sizeOf(std.posix.sockaddr.un));
    try std.posix.listen(listener, 1);
    defer std.fs.deleteFileAbsolute(path) catch {};

    const Server2 = struct {
        fn run(l: std.posix.socket_t, a: std.mem.Allocator) void {
            const conn = std.posix.accept(l, null, null, 0) catch return;
            defer std.posix.close(conn);
            // HELLO exchange first (matches what real executor does).
            const our_hello = pc.encodeHello(a, .{ .rpc_version = pc.rpc_version }) catch return;
            defer a.free(our_hello);
            protocol.writeFrameToFd(conn, our_hello) catch return;
            const peer_hello = protocol.readFrameFromFd(a, conn) catch return;
            a.free(peer_hello);

            // Read the request.
            const req = protocol.readFrameFromFd(a, conn) catch return;
            a.free(req);

            const id: u64 = 13;
            const f1 = pc.encodeProgress(a, id, .{ .tool_call_started = .{ .name = "fly", .args_redacted = "{}" } }) catch return;
            defer a.free(f1);
            protocol.writeFrameToFd(conn, f1) catch return;

            const f2 = pc.encodeProgress(a, id, .{ .agent_response_chunk = .{ .text = "hello world" } }) catch return;
            defer a.free(f2);
            protocol.writeFrameToFd(conn, f2) catch return;

            const f3 = pc.encodeProgress(a, id, .{ .tool_call_completed = .{ .name = "fly", .ms = 42 } }) catch return;
            defer a.free(f3);
            protocol.writeFrameToFd(conn, f3) catch return;

            const terminal = std.json.Stringify.valueAlloc(a, .{
                .jsonrpc = "2.0",
                .id = id,
                .result = .{ .ok = true },
            }, .{}) catch return;
            defer a.free(terminal);
            protocol.writeFrameToFd(conn, terminal) catch return;
        }
    };
    const t = try std.Thread.spawn(.{}, Server2.run, .{ listener, alloc });
    defer t.join();

    var client = Client.init(alloc, path);
    try connectWithRetry(&client);
    defer client.close();

    const Recorder = struct {
        kinds: std.ArrayList(pc.FrameKind) = .{},
        alloc: std.mem.Allocator,

        fn emit(ctx: *anyopaque, frame: pc.ProgressFrame) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.kinds.append(self.alloc, frame.kind()) catch {};
        }
    };
    var rec = Recorder{ .alloc = alloc };
    defer rec.kinds.deinit(alloc);

    const emitter = ProgressEmitter{ .ctx = @ptrCast(&rec), .emit_fn = Recorder.emit };
    var resp = try client.sendRequestStreaming(13, "TestMethod", null, emitter);
    defer resp.deinit();

    try std.testing.expectEqual(@as(usize, 3), rec.kinds.items.len);
    try std.testing.expectEqual(pc.FrameKind.tool_call_started, rec.kinds.items[0]);
    try std.testing.expectEqual(pc.FrameKind.agent_response_chunk, rec.kinds.items[1]);
    try std.testing.expectEqual(pc.FrameKind.tool_call_completed, rec.kinds.items[2]);
    try std.testing.expectEqual(@as(u64, 13), resp.id);
}
