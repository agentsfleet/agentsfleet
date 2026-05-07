//! Dedicated Redis pub/sub SUBSCRIBE connection.
//!
//! Pub/sub blocks the connection: once SUBSCRIBE has been issued, the
//! server pushes unsolicited `["message", channel, payload]` arrays
//! whenever a publisher PUBLISHes, and no other commands may share that
//! socket. The shared request-handler `Client` (mutex-locked req/reply)
//! cannot be used — a Subscriber owns its own TCP stream and Transport.
//!
//! The SSE handler creates one Subscriber per request, calls
//! `subscribe(channel)`, loops on `nextMessage()` until the client
//! disconnects, and `deinit()`s the subscriber on the way out.
//!
//! File-as-struct: this file IS the `Subscriber`.

pub const Subscriber = @This();

alloc: std.mem.Allocator,
transport: redis_transport.Transport,

pub const Message = struct {
    channel: []u8,
    payload: []u8,

    pub fn deinit(self: *Message, alloc: std.mem.Allocator) void {
        alloc.free(self.channel);
        alloc.free(self.payload);
    }
};

pub fn connectFromEnv(alloc: std.mem.Allocator, role: redis_types.RedisRole) !Subscriber {
    const url = try redis_config.resolveRedisUrl(alloc, role);
    defer alloc.free(url);
    return connectFromUrl(alloc, url);
}

fn connectFromUrl(alloc: std.mem.Allocator, url: []const u8) !Subscriber {
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
    errdefer sub.transport.deinit(alloc);

    if (cfg.password) |pwd| {
        if (cfg.username) |usr| {
            try sub.sendCommand(&.{ "AUTH", usr, pwd });
        } else {
            try sub.sendCommand(&.{ "AUTH", pwd });
        }
        var auth = try redis_protocol.readRespValue(alloc, sub.transport.reader());
        defer auth.deinit(alloc);
        try redis_protocol.ensureSimpleOk(auth);
    }

    log.debug("connected", .{ .host = cfg.host, .port = cfg.port, .tls = cfg.use_tls });
    return sub;
}

pub fn deinit(self: *Subscriber) void {
    self.transport.deinit(self.alloc);
}

/// SUBSCRIBE to a single channel and consume the acknowledgment.
/// After this returns, the connection is in subscribe mode — only
/// `nextMessage()` and `unsubscribe()` are valid until disconnect.
pub fn subscribe(self: *Subscriber, channel: []const u8) !void {
    try self.sendCommand(&.{ "SUBSCRIBE", channel });

    var ack = try redis_protocol.readRespValue(self.alloc, self.transport.reader());
    defer ack.deinit(self.alloc);
    try expectSubscribeAck(ack, channel);
}

/// Block until the next pub/sub message arrives. Returns null when the
/// connection is closed cleanly. Caller owns the returned Message.
///
/// In subscribe mode the server emits 3-element arrays:
///   ["message", <channel>, <payload>]
/// All other shapes (PSUBSCRIBE pmessage, ping/pong, subscribe count) are
/// skipped; we keep reading until we see a `message` frame or hit EOF.
pub fn nextMessage(self: *Subscriber) !?Message {
    while (true) {
        var value = redis_protocol.readRespValue(self.alloc, self.transport.reader()) catch |err| switch (err) {
            error.EndOfStream, error.ReadFailed => return null,
            else => return err,
        };
        defer value.deinit(self.alloc);

        if (value != .array) continue;
        const arr = value.array orelse continue;
        if (arr.len < 3) continue;

        const kind = redis_protocol.valueAsString(arr[0]) orelse continue;
        if (!std.mem.eql(u8, kind, "message")) continue;

        const channel = redis_protocol.valueAsString(arr[1]) orelse continue;
        const payload = redis_protocol.valueAsString(arr[2]) orelse continue;

        return Message{
            .channel = try self.alloc.dupe(u8, channel),
            .payload = try self.alloc.dupe(u8, payload),
        };
    }
}

/// UNSUBSCRIBE from all channels. Best-effort — connection is about to close.
pub fn unsubscribe(self: *Subscriber, channel: []const u8) void {
    self.sendCommand(&.{ "UNSUBSCRIBE", channel }) catch return;
}

fn sendCommand(self: *Subscriber, argv: []const []const u8) !void {
    const writer = self.transport.writer();
    try writer.print("*{d}\r\n", .{argv.len});
    for (argv) |arg| {
        try writer.print("${d}\r\n", .{arg.len});
        try writer.writeAll(arg);
        try writer.writeAll("\r\n");
    }
    if (self.transport == .tls) try self.transport.tls.stream_writer.interface.flush();
    try writer.flush();
    if (self.transport == .tls) try self.transport.tls.stream_writer.interface.flush();
}

fn expectSubscribeAck(value: redis_protocol.RespValue, channel: []const u8) !void {
    if (value != .array) return error.RedisSubscribeFailed;
    const arr = value.array orelse return error.RedisSubscribeFailed;
    if (arr.len != 3) return error.RedisSubscribeFailed;
    const kind = redis_protocol.valueAsString(arr[0]) orelse return error.RedisSubscribeFailed;
    if (!std.mem.eql(u8, kind, "subscribe")) return error.RedisSubscribeFailed;
    const channel_echo = redis_protocol.valueAsString(arr[1]) orelse return error.RedisSubscribeFailed;
    if (!std.mem.eql(u8, channel_echo, channel)) return error.RedisSubscribeFailed;
}

const std = @import("std");
const logging = @import("log");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const redis_types = @import("redis_types.zig");
const log = logging.scoped(.redis_subscriber);
