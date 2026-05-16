//! Redis client. File-as-struct: the file IS the `Client` type.

pub const Client = @This();

const S_PONG = "PONG";
const S_SET = "SET";
const S_AUTH = "AUTH";
const S_XADD_ZOMBIE_EVENT_FAILED = "xadd_zombie_event_failed";
const S_EX = "EX";
const S_D = "{d}";
const S_PING = "PING";
const S_OK = "OK";

alloc: std.mem.Allocator,
cfg: redis_config.Config,
transport: redis_transport.Transport,
lock: std.Thread.Mutex = .{},

pub fn connectFromEnv(alloc: std.mem.Allocator, role: redis_types.RedisRole) !Client {
    const url_owned = try redis_config.resolveRedisUrl(alloc, role);
    defer alloc.free(url_owned);
    return connectFromUrl(alloc, url_owned);
}

pub fn connectFromUrl(alloc: std.mem.Allocator, url: []const u8) !Client {
    const cfg = try redis_config.parseRedisUrl(alloc, url);
    errdefer redis_config.deinitConfig(alloc, cfg);

    // SAFETY: written by surrounding init logic before any read of this storage.
    var client = Client{ .alloc = alloc, .cfg = cfg, .transport = undefined };
    client.transport = try dialAndAuth(alloc, cfg);
    log.info("connected", .{ .host = cfg.host, .port = cfg.port, .tls = cfg.use_tls });
    return client;
}

pub fn deinit(self: *Client) void {
    self.transport.deinit(self.alloc);
    redis_config.deinitConfig(self.alloc, self.cfg);
}

pub fn publish(self: *Client, channel: []const u8, data: []const u8) !void {
    var resp = try self.command(&.{ "PUBLISH", channel, data });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .integer => {},
        else => return error.RedisPublishFailed,
    }
    log.debug("publish", .{ .channel = channel, .data_len = data.len });
}

/// SET key value EX ttl_seconds — used for cancellation signals.
pub fn setEx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !void {
    var ttl_buf: [16]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, S_D, .{ttl_seconds});
    var resp = try self.command(&.{ S_SET, key, value, S_EX, ttl_str });
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_OK)) return error.RedisSetExFailed,
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
    var resp = try self.command(&.{S_PING});
    defer resp.deinit(self.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_PONG)) return error.RedisPingFailed,
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
    //
    // PING is idempotent, so we retry the full round-trip (write AND read)
    // after a forced reconnect. `commandUnlocked` only retries write-phase
    // failures because most callers can't tolerate a duplicate request; here
    // we know we can.
    self.ping() catch {
        self.lock.lock();
        defer self.lock.unlock();
        try self.reconnectUnlocked();
        var resp = try self.commandUnlocked(&.{S_PING});
        defer resp.deinit(self.alloc);
        switch (resp) {
            .simple => |v| if (!std.mem.eql(u8, v, S_PONG)) return error.RedisPingFailed,
            else => return error.RedisPingFailed,
        }
    };
}

pub fn aclWhoAmI(self: *Client) ![]u8 {
    var resp = try self.command(&.{ "ACL", "WHOAMI" });
    defer resp.deinit(self.alloc);
    const who = redis_protocol.valueAsString(resp) orelse return error.RedisUnexpectedResponse;
    return self.alloc.dupe(u8, who);
}

/// setNx sets key=value with ttl only if key does not exist.
/// Returns true if key was set (new), false if key already existed (duplicate).
pub fn setNx(self: *Client, key: []const u8, value: []const u8, ttl_seconds: u32) !bool {
    var ttl_buf: [12]u8 = undefined;
    const ttl_str = try std.fmt.bufPrint(&ttl_buf, S_D, .{ttl_seconds});
    var resp = try self.commandAllowError(&.{ S_SET, key, value, "NX", S_EX, ttl_str });
    defer resp.deinit(self.alloc);
    return switch (resp) {
        .simple => |s| std.mem.eql(u8, s, S_OK),
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
            log.err(S_XADD_ZOMBIE_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = envelope.zombie_id, .actor = envelope.actor });
            return error.RedisXaddFailed;
        },
        else => {
            log.err(S_XADD_ZOMBIE_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = envelope.zombie_id, .actor = envelope.actor });
            return error.RedisXaddFailed;
        },
    };
    const owned_id = try self.alloc.dupe(u8, id_str);
    log.debug("xadd_zombie_event", .{ .zombie_id = envelope.zombie_id, .event_id = owned_id, .actor = envelope.actor, .type = envelope.event_type.toSlice() });
    return owned_id;
}

pub fn command(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
    var value = try self.commandAllowError(argv);
    if (value == .err) {
        log.err("command_error", .{ .cmd = if (argv.len > 0) argv[0] else "unknown", .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED });
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
    // Write-phase failures (BrokenPipe, ConnectionResetByPeer, etc.) prove the
    // server hadn't seen the request — safe to reconnect+resend for any cmd.
    // Read-phase failures are NOT auto-retried here because the server may have
    // already processed the write; replaying could double-XADD or double-PUBLISH.
    // Idempotent callers (e.g. /readyz PING) layer their own read-phase retry.
    writeArgvToTransport(&self.transport, argv) catch |err| {
        log.warn("write_failed_reconnecting", .{ .err = @errorName(err) });
        self.reconnectUnlocked() catch return err;
        try writeArgvToTransport(&self.transport, argv);
        return redis_protocol.readRespValue(self.alloc, self.transport.reader());
    };
    return redis_protocol.readRespValue(self.alloc, self.transport.reader());
}

fn writeArgvToTransport(transport: *redis_transport.Transport, argv: []const []const u8) !void {
    const writer = transport.writer();
    try writer.print("*{d}\r\n", .{argv.len});
    for (argv) |arg| {
        try writer.print("${d}\r\n", .{arg.len});
        try writer.writeAll(arg);
        try writer.writeAll("\r\n");
    }
    if (transport.* == .tls) try transport.tls.stream_writer.interface.flush();
    try writer.flush();
    if (transport.* == .tls) try transport.tls.stream_writer.interface.flush();
}

/// Build a fresh authenticated transport. Used at first connect and on
/// transparent reconnect. On failure the partially-built transport is freed
/// before returning.
fn dialAndAuth(alloc: std.mem.Allocator, cfg: redis_config.Config) !redis_transport.Transport {
    const stream = try std.net.tcpConnectToHost(alloc, cfg.host, cfg.port);

    // SAFETY: written by surrounding init logic before any read of this storage.
    var transport: redis_transport.Transport = undefined;
    if (cfg.use_tls) {
        // SAFETY: written by surrounding init logic before any read of this storage.
        transport = .{ .tls = undefined };
        try transport.tls.initInPlace(alloc, stream, cfg.host);
    } else {
        transport = .{ .plain = try redis_transport.PlainTransport.init(alloc, stream) };
    }
    errdefer transport.deinit(alloc);

    if (cfg.password) |pwd| {
        const argv: []const []const u8 = if (cfg.username) |usr|
            &.{ S_AUTH, usr, pwd }
        else
            &.{ S_AUTH, pwd };
        try writeArgvToTransport(&transport, argv);
        var resp = try redis_protocol.readRespValue(alloc, transport.reader());
        defer resp.deinit(alloc);
        try redis_protocol.ensureSimpleOk(resp);
    }
    return transport;
}

/// Re-dial Redis using the stored config and atomically replace the live
/// transport. Caller must hold `self.lock`. If the dial or re-AUTH fails the
/// existing transport is left untouched so the next attempt can retry.
fn reconnectUnlocked(self: *Client) !void {
    const new_transport = try dialAndAuth(self.alloc, self.cfg);
    self.transport.deinit(self.alloc);
    self.transport = new_transport;
    log.info("reconnected", .{ .host = self.cfg.host, .port = self.cfg.port });
}

pub fn makeConsumerId(alloc: std.mem.Allocator) ![]u8 {
    var host_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    const host = std.posix.gethostname(&host_buf) catch "localhost";
    const now = std.time.nanoTimestamp();
    return std.fmt.allocPrint(alloc, "{s}-{s}-{d}", .{ queue_consts.consumer_prefix, host, now });
}

const std = @import("std");
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const error_codes = @import("../errors/error_registry.zig");
const EventEnvelope = @import("../zombie/event_envelope.zig");
const log = logging.scoped(.redis_queue);
