//! Redis pub/sub subscriber. Uses a dedicated connection (SUBSCRIBE puts
//! a Redis connection into subscriber-only mode — no other commands allowed).

const std = @import("std");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const redis_types = @import("redis_types.zig");

const log = std.log.scoped(.redis_pubsub);

/// Read timeout for pub/sub subscriber socket (ms). Must fire before the
/// 30-second proxy idle timeout so the heartbeat loop can emit `: heartbeat`.
pub const PUBSUB_READ_TIMEOUT_MS: u32 = 25_000;

pub const PubSubMessage = struct {
    channel: []u8,
    data: []u8,
    alloc: std.mem.Allocator,

    pub fn deinit(self: *PubSubMessage) void {
        self.alloc.free(self.channel);
        self.alloc.free(self.data);
    }
};

pub const Subscriber = struct {
    alloc: std.mem.Allocator,
    transport: redis_transport.Transport,

    pub fn connectFromEnv(alloc: std.mem.Allocator) !Subscriber {
        const url_owned = try redis_config.resolveRedisUrl(alloc, .worker);
        defer alloc.free(url_owned);
        return connectFromUrl(alloc, url_owned);
    }

    pub fn connectFromUrl(alloc: std.mem.Allocator, url: []const u8) !Subscriber {
        const cfg = try redis_config.parseRedisUrl(alloc, url);
        defer redis_config.deinitConfig(alloc, cfg);

        const stream = try std.net.tcpConnectToHost(alloc, cfg.host, cfg.port);
        var sub = Subscriber{ .alloc = alloc, .transport = undefined };

        if (cfg.use_tls) {
            sub.transport = .{ .tls = undefined };
            try sub.transport.tls.initInPlace(alloc, stream, cfg.host);
        } else {
            sub.transport = .{ .plain = try redis_transport.PlainTransport.init(alloc, stream) };
        }
        errdefer sub.deinit();

        if (cfg.password) |pwd| {
            if (cfg.username) |usr| {
                try sub.sendCommand(&.{ "AUTH", usr, pwd });
            } else {
                try sub.sendCommand(&.{ "AUTH", pwd });
            }
            var resp = try redis_protocol.readRespValue(alloc, sub.transport.reader());
            defer resp.deinit(alloc);
            try redis_protocol.ensureSimpleOk(resp);
        }

        log.info("redis_pubsub.connected host={s} port={d} tls={}", .{ cfg.host, cfg.port, cfg.use_tls });
        return sub;
    }

    pub fn deinit(self: *Subscriber) void {
        self.transport.deinit(self.alloc);
    }

    pub fn subscribe(self: *Subscriber, channel: []const u8) !void {
        try self.sendCommand(&.{ "SUBSCRIBE", channel });
        var resp = try redis_protocol.readRespValue(self.alloc, self.transport.reader());
        defer resp.deinit(self.alloc);

        // Set SO_RCVTIMEO after subscription confirmation so readMessage()
        // returns periodically, allowing the heartbeat loop to fire.
        self.setReadTimeout(PUBSUB_READ_TIMEOUT_MS);
        log.debug("redis_pubsub.subscribed channel={s}", .{channel});
    }

    /// Set SO_RCVTIMEO on the underlying socket so reads time out after `ms` milliseconds.
    fn setReadTimeout(self: *Subscriber, ms: u32) void {
        const fd = switch (self.transport) {
            .plain => |p| p.stream.handle,
            .tls => |t| t.stream.handle,
        };
        const timeout = std.posix.timeval{
            .sec = @intCast(ms / 1000),
            .usec = @intCast((ms % 1000) * 1000),
        };
        std.posix.setsockopt(fd, std.posix.SOL.SOCKET, std.posix.SO.RCVTIMEO, std.mem.asBytes(&timeout)) catch |err| {
            log.warn("redis_pubsub.setsockopt_fail err={s}", .{@errorName(err)});
        };
    }

    /// Block until the next pub/sub message arrives or the socket read times out.
    /// Returns null on timeout (WouldBlock / ConnectionTimedOut) — not a fatal error.
    /// Propagates fatal errors (ConnectionResetByPeer, BrokenPipe) so the caller
    /// can break the loop and fall back to DB polling.
    /// Caller owns the returned PubSubMessage and must call deinit().
    pub fn readMessage(self: *Subscriber) !?PubSubMessage {
        var resp = redis_protocol.readRespValue(self.alloc, self.transport.reader()) catch |err| {
            // SO_RCVTIMEO timeout surfaces as ReadFailed through both the plain
            // TCP Io.Reader and the TLS Io.Reader (Zig 0.15.2 wraps WouldBlock
            // into ReadFailed at the Io.Reader layer). Return null so the stream
            // handler can emit a heartbeat and continue the loop.
            //
            // EndOfStream means the connection was closed cleanly — also not fatal
            // in the pub/sub context (server may have disconnected).
            //
            // Any other error type is unexpected and propagated.
            if (err == error.ReadFailed or err == error.EndOfStream) return null;
            log.warn("redis_pubsub.read_fatal err={s}", .{@errorName(err)});
            return err;
        };
        defer resp.deinit(self.alloc);

        // Pub/sub message format: array of 3 elements: ["message", channel, data]
        if (resp != .array) return null;
        const arr = resp.array orelse return null;
        if (arr.len != 3) return null;

        const kind = redis_protocol.valueAsString(arr[0]) orelse return null;
        if (!std.mem.eql(u8, kind, "message")) return null;

        const channel_raw = redis_protocol.valueAsString(arr[1]) orelse return null;
        const data_raw = redis_protocol.valueAsString(arr[2]) orelse return null;

        return .{
            .channel = try self.alloc.dupe(u8, channel_raw),
            .data = try self.alloc.dupe(u8, data_raw),
            .alloc = self.alloc,
        };
    }

    fn sendCommand(self: *Subscriber, argv: []const []const u8) !void {
        const writer = self.transport.writer();
        try writer.print("*{d}\r\n", .{argv.len});
        for (argv) |arg| {
            try writer.print("${d}\r\n", .{arg.len});
            try writer.writeAll(arg);
            try writer.writeAll("\r\n");
        }
        try writer.flush();
        // For TLS: writer.flush() encrypts into stream_writer's buffer but does not
        // send it to the socket. Must flush the underlying socket writer explicitly.
        // Plain transport: writer.flush() goes directly to the socket — no extra step.
        if (self.transport == .tls) try self.transport.tls.stream_writer.interface.flush();
    }
};
