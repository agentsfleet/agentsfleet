//! Redis client. File-as-struct: the file IS the `Client` type.

pub const Client = @This();

alloc: std.mem.Allocator,
transport: redis_transport.Transport,
lock: std.Thread.Mutex = .{},

pub fn connectFromEnv(alloc: std.mem.Allocator, role: redis_types.RedisRole) !Client {
    const url_owned = try redis_config.resolveRedisUrl(alloc, role);
    defer alloc.free(url_owned);
    return connectFromUrl(alloc, url_owned);
}

pub fn connectFromUrl(alloc: std.mem.Allocator, url: []const u8) !Client {
    const cfg = try redis_config.parseRedisUrl(alloc, url);
    defer redis_config.deinitConfig(alloc, cfg);

    const stream = try std.net.tcpConnectToHost(alloc, cfg.host, cfg.port);
    var client = Client{ .alloc = alloc, .transport = undefined };

    if (cfg.use_tls) {
        client.transport = .{ .tls = undefined };
        try client.transport.tls.initInPlace(alloc, stream, cfg.host);
    } else {
        client.transport = .{ .plain = try redis_transport.PlainTransport.init(alloc, stream) };
    }
    errdefer client.deinit();

    if (cfg.password) |pwd| {
        if (cfg.username) |usr| {
            var auth = try client.commandUnlocked(&.{ "AUTH", usr, pwd });
            defer auth.deinit(alloc);
            try redis_protocol.ensureSimpleOk(auth);
        } else {
            var auth = try client.commandUnlocked(&.{ "AUTH", pwd });
            defer auth.deinit(alloc);
            try redis_protocol.ensureSimpleOk(auth);
        }
    }

    log.info("redis.connected host={s} port={d} tls={}", .{ cfg.host, cfg.port, cfg.use_tls });
    return client;
}

pub fn deinit(self: *Client) void {
    self.transport.deinit(self.alloc);
}

pub fn publish(self: *Client, channel: []const u8, data: []const u8) !void {
    var resp = try self.command(&.{ "PUBLISH", channel, data });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .integer => {},
        else => return error.RedisPublishFailed,
    }
    log.debug("redis.publish channel={s} data_len={d}", .{ channel, data.len });
}

/// SET key value EX ttl_seconds — used for cancellation signals.
pub fn setEx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !void {
    var ttl_buf: [16]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, "{d}", .{ttl_seconds});
    var resp = try self.command(&.{ "SET", key, value, "EX", ttl_str });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.RedisSetExFailed,
        else => return error.RedisSetExFailed,
    }
}

pub fn exists(self: *Client, key: []const u8) !bool {
    var resp = try self.command(&.{ "EXISTS", key });
    defer resp.deinit(self.alloc);
    return switch (resp) {
        .integer => |n| n > 0,
        else => error.RedisExistsFailed,
    };
}

pub fn ping(self: *Client) !void {
    var resp = try self.command(&.{"PING"});
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "PONG")) return error.RedisPingFailed,
        else => return error.RedisPingFailed,
    }
}

pub fn readyCheck(self: *Client) !void {
    // PING-only: the API server doesn't consume any stream itself, so a
    // consumer-group probe (which would also require XGROUP CREATE perms in
    // role-based-ACL Redis) adds no signal. Per-zombie streams + groups are
    // created lazily by the worker via `redis_zombie.ensureZombieEventsGroup`
    // when a zombie spawns; the control stream by `control_stream.ensureGroup`
    // at worker boot.
    try self.ping();
}

pub fn aclWhoAmI(self: *Client) ![]u8 {
    var resp = try self.command(&.{ "ACL", "WHOAMI" });
    defer resp.deinit(self.alloc);
    const who = redis_protocol.valueAsString(resp) orelse return error.RedisUnexpectedResponse;
    return try self.alloc.dupe(u8, who);
}

/// setNx sets key=value with ttl only if key does not exist.
/// Returns true if key was set (new), false if key already existed (duplicate).
pub fn setNx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !bool {
    var ttl_buf: [12]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, "{d}", .{ttl_seconds});
    var resp = try self.commandAllowError(&.{ "SET", key, value, "NX", "EX", ttl_str });
    defer resp.deinit(self.alloc);
    return switch (resp) {
        .simple => |s| std.mem.eql(u8, s, "OK"),
        else => false,
    };
}

/// XADD an EventEnvelope onto `zombie:{envelope.zombie_id}:events`. The Redis
/// stream entry id IS the canonical event_id; this function returns it
/// allocated via `self.alloc` so the caller (e.g. `POST /messages`) can
/// surface it in the response body for SSE correlation.
///
/// Stream is trimmed approximately to MAXLEN 10000 entries.
pub fn xaddZombieEvent(self: *Client, envelope: EventEnvelope) ![]u8 {
    var stream_key_buf: [128]u8 = undefined;
    const stream_key = try std.fmt.bufPrint(&stream_key_buf, "zombie:{s}:events", .{envelope.zombie_id});

    const payload_argv = try envelope.encodeForXAdd(self.alloc);
    defer EventEnvelope.freeXAddArgv(self.alloc, payload_argv);

    var argv = try self.alloc.alloc([]const u8, 6 + payload_argv.len);
    defer self.alloc.free(argv);
    argv[0] = "XADD";
    argv[1] = stream_key;
    argv[2] = "MAXLEN";
    argv[3] = "~";
    argv[4] = "10000";
    argv[5] = "*";
    @memcpy(argv[6..], payload_argv);

    var resp = try self.command(argv);
    defer resp.deinit(self.alloc);

    const id_str = switch (resp) {
        .bulk => |v| v orelse {
            log.err("redis.xadd_zombie_event_fail zombie_id={s} actor={s}", .{ envelope.zombie_id, envelope.actor });
            return error.RedisXaddFailed;
        },
        else => {
            log.err("redis.xadd_zombie_event_fail zombie_id={s} actor={s}", .{ envelope.zombie_id, envelope.actor });
            return error.RedisXaddFailed;
        },
    };
    const owned_id = try self.alloc.dupe(u8, id_str);
    log.debug("redis.xadd_zombie_event zombie_id={s} event_id={s} actor={s} type={s}", .{ envelope.zombie_id, owned_id, envelope.actor, envelope.event_type.toSlice() });
    return owned_id;
}

pub fn command(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
    var value = try self.commandAllowError(argv);
    if (value == .err) {
        log.err("redis.command_error cmd={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{if (argv.len > 0) argv[0] else "unknown"});
        value.deinit(self.alloc);
        return error.RedisCommandError;
    }
    return value;
}

pub fn commandAllowError(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
    self.lock.lock();
    defer self.lock.unlock();
    return self.commandUnlocked(argv);
}

fn commandUnlocked(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
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

    const reader = self.transport.reader();
    return try redis_protocol.readRespValue(self.alloc, reader);
}

pub fn makeConsumerId(alloc: std.mem.Allocator) ![]u8 {
    var host_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    const host = std.posix.gethostname(&host_buf) catch "localhost";
    const now = std.time.nanoTimestamp();
    return std.fmt.allocPrint(alloc, "{s}-{s}-{d}", .{ queue_consts.consumer_prefix, host, now });
}

const std = @import("std");
const queue_consts = @import("constants.zig");
const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const error_codes = @import("../errors/error_registry.zig");
const EventEnvelope = @import("../zombie/event_envelope.zig");
const log = std.log.scoped(.redis_queue);
