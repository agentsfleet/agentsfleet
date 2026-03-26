const std = @import("std");
const queue_consts = @import("constants.zig");
const redis_types = @import("redis_types.zig");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const error_codes = @import("../errors/codes.zig");

const log = std.log.scoped(.redis_queue);

pub const Client = struct {
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

    pub fn ping(self: *Client) !void {
        var resp = try self.command(&.{"PING"});
        defer resp.deinit(self.alloc);
        switch (resp) {
            .simple => |v| if (!std.mem.eql(u8, v, "PONG")) return error.RedisPingFailed,
            else => return error.RedisPingFailed,
        }
    }

    pub fn ensureConsumerGroup(self: *Client) !void {
        var resp = try self.commandAllowError(&.{
            "XGROUP",
            "CREATE",
            queue_consts.stream_name,
            queue_consts.consumer_group,
            "0",
            "MKSTREAM",
        });
        defer resp.deinit(self.alloc);

        switch (resp) {
            .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.RedisGroupCreateFailed,
            .err => |msg| {
                if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
                return error.RedisGroupCreateFailed;
            },
            else => return error.RedisGroupCreateFailed,
        }
    }

    pub fn readyCheck(self: *Client) !void {
        try self.ping();
        try self.ensureConsumerGroup();
    }

    pub fn aclWhoAmI(self: *Client) ![]u8 {
        var resp = try self.command(&.{ "ACL", "WHOAMI" });
        defer resp.deinit(self.alloc);
        const who = redis_protocol.valueAsString(resp) orelse return error.RedisUnexpectedResponse;
        return try self.alloc.dupe(u8, who);
    }

    pub fn xaddRun(self: *Client, run_id: []const u8, attempt: u32, workspace_id: []const u8) !void {
        var attempt_buf: [16]u8 = undefined;
        const attempt_str = try std.fmt.bufPrint(&attempt_buf, "{d}", .{attempt});
        var resp = try self.command(&.{
            "XADD",
            queue_consts.stream_name,
            "*",
            queue_consts.field_run_id,
            run_id,
            queue_consts.field_attempt,
            attempt_str,
            queue_consts.field_workspace_id,
            workspace_id,
        });
        defer resp.deinit(self.alloc);

        switch (resp) {
            .bulk => |v| if (v == null) {
                log.err("redis.xadd_fail run_id={s} attempt={d} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{ run_id, attempt });
                return error.RedisXaddFailed;
            },
            else => {
                log.err("redis.xadd_fail run_id={s} attempt={d} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{ run_id, attempt });
                return error.RedisXaddFailed;
            },
        }
        log.debug("redis.xadd run_id={s} attempt={d} workspace_id={s}", .{ run_id, attempt, workspace_id });
    }

    pub fn xack(self: *Client, message_id: []const u8) !void {
        var resp = try self.command(&.{
            "XACK",
            queue_consts.stream_name,
            queue_consts.consumer_group,
            message_id,
        });
        defer resp.deinit(self.alloc);

        switch (resp) {
            .integer => |v| if (v < 0) {
                log.err("redis.xack_fail message_id={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{message_id});
                return error.RedisXackFailed;
            },
            else => {
                log.err("redis.xack_fail message_id={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{message_id});
                return error.RedisXackFailed;
            },
        }
        log.debug("redis.xack message_id={s}", .{message_id});
    }

    pub fn xreadgroupOne(self: *Client, consumer_id: []const u8) !?redis_types.QueueMessage {
        var resp = try self.command(&.{
            "XREADGROUP",
            "GROUP",
            queue_consts.consumer_group,
            consumer_id,
            "COUNT",
            queue_consts.xread_count,
            "BLOCK",
            queue_consts.xread_block_ms,
            "STREAMS",
            queue_consts.stream_name,
            ">",
        });
        defer resp.deinit(self.alloc);
        return self.decodeSingleMessage(resp);
    }

    pub fn xautoclaimOne(self: *Client, consumer_id: []const u8) !?redis_types.QueueMessage {
        var resp = try self.command(&.{
            "XAUTOCLAIM",
            queue_consts.stream_name,
            queue_consts.consumer_group,
            consumer_id,
            queue_consts.xautoclaim_min_idle_ms,
            queue_consts.xautoclaim_start,
            "COUNT",
            queue_consts.xautoclaim_count,
        });
        defer resp.deinit(self.alloc);
        return self.decodeAutoClaimMessage(resp);
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

    fn commandAllowError(self: *Client, argv: []const []const u8) !redis_protocol.RespValue {
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

    fn decodeSingleMessage(self: *Client, value: redis_protocol.RespValue) !?redis_types.QueueMessage {
        if (value != .array) return null;
        const top = value.array orelse return null;
        if (top.len == 0) return null;
        if (top.len != 1) return error.RedisUnexpectedResponse;
        if (top[0] != .array) return error.RedisUnexpectedResponse;
        const stream_entry = top[0].array orelse return error.RedisUnexpectedResponse;
        if (stream_entry.len != 2) return error.RedisUnexpectedResponse;
        if (stream_entry[1] != .array) return error.RedisUnexpectedResponse;
        const messages = stream_entry[1].array orelse return null;
        if (messages.len == 0) return null;
        const message = try self.decodeMessageTuple(messages[0]);
        return message;
    }

    fn decodeAutoClaimMessage(self: *Client, value: redis_protocol.RespValue) !?redis_types.QueueMessage {
        if (value != .array) return null;
        const top = value.array orelse return null;
        if (top.len < 2) return error.RedisUnexpectedResponse;
        if (top[1] != .array) return error.RedisUnexpectedResponse;
        const messages = top[1].array orelse return null;
        if (messages.len == 0) return null;
        const message = try self.decodeMessageTuple(messages[0]);
        return message;
    }

    fn decodeMessageTuple(self: *Client, item: redis_protocol.RespValue) !redis_types.QueueMessage {
        if (item != .array) return error.RedisUnexpectedResponse;
        const tuple = item.array orelse return error.RedisUnexpectedResponse;
        if (tuple.len != 2) return error.RedisUnexpectedResponse;
        const msg_id = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

        if (tuple[1] != .array) return error.RedisUnexpectedResponse;
        const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
        if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

        var run_id: ?[]u8 = null;
        var workspace_id: ?[]u8 = null;
        var attempt: u32 = 0;

        var i: usize = 0;
        while (i < fields.len) : (i += 2) {
            const key = redis_protocol.valueAsString(fields[i]) orelse continue;
            const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;

            if (std.mem.eql(u8, key, queue_consts.field_run_id)) {
                run_id = try self.alloc.dupe(u8, val);
            } else if (std.mem.eql(u8, key, queue_consts.field_workspace_id)) {
                workspace_id = try self.alloc.dupe(u8, val);
            } else if (std.mem.eql(u8, key, queue_consts.field_attempt)) {
                attempt = std.fmt.parseInt(u32, val, 10) catch 0;
            }
        }

        if (run_id == null) return error.RedisUnexpectedResponse;

        return .{
            .message_id = try self.alloc.dupe(u8, msg_id),
            .run_id = run_id.?,
            .attempt = attempt,
            .workspace_id = workspace_id,
        };
    }
};

pub fn makeConsumerId(alloc: std.mem.Allocator) ![]u8 {
    var host_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    const host = std.posix.gethostname(&host_buf) catch "localhost";
    const now = std.time.nanoTimestamp();
    return std.fmt.allocPrint(alloc, "{s}-{s}-{d}", .{ queue_consts.consumer_prefix, host, now });
}

