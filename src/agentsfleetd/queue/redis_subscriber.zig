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
const Self = @This();

const S_AUTH = "AUTH";
const S_SUBSCRIBE = "SUBSCRIBE";
const S_UNSUBSCRIBE = "UNSUBSCRIBE";

alloc: std.mem.Allocator,
/// Io backing this subscriber's socket (Zig 0.16 Stream ops take Io).
io: std.Io,
transport: redis_transport.Transport,
read_timeout_ms: ?u32,

pub const Message = struct {
    channel: []u8,
    payload: []u8,

    pub fn deinit(self: *Message, alloc: std.mem.Allocator) void {
        alloc.free(self.channel);
        alloc.free(self.payload);
        self.* = undefined;
    }
};

/// Per-subscriber config. `read_timeout_ms` non-null installs `SO_RCVTIMEO`
/// after the SUBSCRIBE ack so `nextMessage` returns `null` on read timeout
/// (used by SSE handlers and the test harness to interleave heartbeats /
/// budget checks). Null = block forever (default for callers that drive
/// their own deadline externally).
pub const InitOptions = struct {
    read_timeout_ms: ?u32 = null,
    /// Custom TLS CA bundle path → `Config.ca_cert_file`. Test harnesses pass the
    /// broker's self-signed cert; the env-map path resolves it from
    /// `REDIS_TLS_CA_CERT_FILE` instead. Null = system trust store.
    ca_cert_file: ?[]const u8 = null,
};

pub fn connectFromEnv(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, role: redis_types.RedisRole, options: InitOptions) !Self {
    const url = try redis_config.resolveRedisUrl(env_map, alloc, role);
    defer alloc.free(url);
    var cfg = try redis_config.parseRedisUrl(alloc, url);
    defer redis_config.deinitConfig(alloc, cfg);
    cfg.ca_cert_file = try common.env.owned(env_map, alloc, redis_config.CA_CERT_FILE_ENV);
    return connectFromConfig(io, alloc, cfg, options, null);
}

pub fn connectFromUrl(io: std.Io, alloc: std.mem.Allocator, url: []const u8, options: InitOptions) !Self {
    var cfg = try redis_config.parseRedisUrl(alloc, url);
    defer redis_config.deinitConfig(alloc, cfg);
    if (options.ca_cert_file) |ca| cfg.ca_cert_file = try alloc.dupe(u8, ca);
    return connectFromConfig(io, alloc, cfg, options, null);
}

/// One absolute boot-clock budget for a whole connection attempt: resolve,
/// dial, TLS handshake, and AUTH all spend from it, so a stall in any stage
/// cannot reset the allowance the way per-stage timeouts would.
///
/// The owner is CALLER-owned and address-stable. The caller arms a scheduler
/// guard on it before calling connect and finishes that guard before moving the
/// returned Subscriber — a Subscriber is returned by value, so its own storage
/// is not a safe interrupt target.
pub const SetupBudget = struct {
    owner: *call_deadline.SocketOwner,
    generation: u64,
    /// Absolute boot-clock nanoseconds.
    deadline_ns: i96,
};

/// Setup stalled past its budget. Distinct from an auth rejection and from an
/// ordinary transport failure so a caller can retry only what is retryable.
pub const SetupError = error{RedisSetupTimedOut};

/// Dial a dedicated pub/sub connection from an already-resolved config. `cfg`
/// is BORROWED (e.g. the request-path Client's pool config) — read during the
/// dial, never freed here. This is the SSE path's seam: it reuses the pool's
/// resolved config instead of re-reading env.
///
/// `budget` null keeps the historical unbounded dial for callers that drive
/// their own deadline; the hub always supplies one.
pub fn connectFromConfig(
    io: std.Io,
    alloc: std.mem.Allocator,
    cfg: redis_config.Config,
    options: InitOptions,
    budget: ?SetupBudget,
) !Self {
    // Zig 0.16 dial: resolve host (DNS) + connect via Io.net.HostName. Name
    // resolution has no socket yet, so shutdown cannot interrupt it — the dial
    // is raced against the budget instead, and the loser is canceled.
    const hostname = try std.Io.net.HostName.init(cfg.host);
    const stream = if (budget) |b|
        try dialWithinBudget(io, hostname, cfg.port, b.deadline_ns)
    else
        try hostname.connect(io, cfg.port, .{ .mode = .stream });

    // The attempt owns the socket the moment one exists, so every later stage
    // is interruptible even though the dial was not.
    if (budget) |b| _ = b.owner.attachSocket(b.generation, stream.socket.handle);

    // SAFETY: written by surrounding init logic before any read of this storage.
    var sub = Self{ .alloc = alloc, .io = io, .transport = undefined, .read_timeout_ms = options.read_timeout_ms };

    if (cfg.use_tls) {
        // SAFETY: written by surrounding init logic before any read of this storage.
        sub.transport = .{ .tls = undefined };
        try sub.transport.tls.initInPlace(io, alloc, stream, cfg.host, cfg.ca_cert_file);
    } else {
        sub.transport = .{ .plain = try redis_transport.PlainTransport.init(io, alloc, stream) };
    }
    errdefer sub.transport.deinit(io, alloc);
    try checkBudget(budget);

    if (cfg.password) |pwd| {
        if (cfg.username) |usr| {
            try sub.sendCommand(&.{ S_AUTH, usr, pwd });
        } else {
            try sub.sendCommand(&.{ S_AUTH, pwd });
        }
        var auth = try redis_protocol.readRespValue(alloc, sub.transport.reader());
        defer auth.deinit(alloc);
        try checkBudget(budget);
        try redis_protocol.ensureSimpleOk(auth);
    }

    log.debug("connected", .{ .host = cfg.host, .port = cfg.port, .tls = cfg.use_tls });
    return sub;
}

/// Between-stage check: the deadline fired and shut the socket down, so the
/// stage that just returned did so because of us, not the peer.
fn checkBudget(budget: ?SetupBudget) SetupError!void {
    const b = budget orelse return;
    if (b.owner.wasInterrupted()) return SetupError.RedisSetupTimedOut;
}

/// Race resolve+dial against the shared budget and cancel the loser, so a
/// black-holed DNS server or a SYN that never completes cannot outlive it.
fn dialWithinBudget(
    io: std.Io,
    hostname: std.Io.net.HostName,
    port: u16,
    deadline_ns: i96,
) !std.Io.net.Stream {
    if (std.Io.Clock.boot.now(io).toNanoseconds() >= deadline_ns) return SetupError.RedisSetupTimedOut;

    const Selected = union(enum) {
        dial: std.Io.net.HostName.ConnectError!std.Io.net.Stream,
        timeout: std.Io.Cancelable!void,
    };
    var result_buf: [2]Selected = undefined;
    var select = std.Io.Select(Selected).init(io, &result_buf);
    try select.concurrent(.timeout, waitForDeadline, .{ io, deadline_ns });
    select.concurrent(.dial, dialTask, .{ io, hostname, port }) catch |err| {
        select.cancelDiscard();
        return err;
    };
    const selected = select.await() catch |err| {
        select.cancelDiscard();
        return err;
    };
    // Joins the loser before returning: no dial helper outlives this call.
    select.cancelDiscard();
    return switch (selected) {
        .dial => |result| result,
        .timeout => |result| {
            result catch |err| switch (err) {
                error.Canceled => {},
            };
            return SetupError.RedisSetupTimedOut;
        },
    };
}

fn dialTask(io: std.Io, hostname: std.Io.net.HostName, port: u16) std.Io.net.HostName.ConnectError!std.Io.net.Stream {
    return hostname.connect(io, port, .{ .mode = .stream });
}

fn waitForDeadline(io: std.Io, deadline_ns: i96) std.Io.Cancelable!void {
    const deadline = std.Io.Timestamp.fromNanoseconds(deadline_ns).withClock(.boot);
    try deadline.wait(io);
}

pub fn deinit(self: *Self) void {
    self.transport.deinit(self.io, self.alloc);
    self.* = undefined;
}

/// SUBSCRIBE to a single channel and consume the acknowledgment.
/// After this returns, the connection is in subscribe mode — only
/// `nextMessage()` and `unsubscribe()` are valid until disconnect.
pub fn subscribe(self: *Self, channel: []const u8) !void {
    try self.sendCommand(&.{ S_SUBSCRIBE, channel });

    var ack = try redis_protocol.readRespValue(self.alloc, self.transport.reader());
    defer ack.deinit(self.alloc);
    try expectSubscribeAck(ack, channel);

    // Install SO_RCVTIMEO post-ack so the AUTH/SUBSCRIBE handshakes are not
    // exposed to it. Per-Connection read timeout means `nextMessage` returns
    // null on timeout (see swallow in nextMessage).
    if (self.read_timeout_ms) |ms| self.transport.setReadTimeout(ms);
}

/// Block until the next pub/sub message arrives. Returns null when the
/// connection is closed cleanly. Caller owns the returned Message.
///
/// In subscribe mode the server emits 3-element arrays:
///   ["message", <channel>, <payload>]
/// All other shapes (PSUBSCRIBE pmessage, ping/pong, subscribe count) are
/// skipped; we keep reading until we see a `message` frame or hit EOF.
pub fn nextMessage(self: *Self) !?Message {
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

        // Order matters: if the second dupe fails with OOM, the first dupe
        // must be freed before we propagate. A naked struct-init `.{ .a =
        // try dupe, .b = try dupe }` leaks `a` when `b` fails — Zig
        // evaluates field exprs left-to-right but doesn't unwind the first
        // allocation when the second errors out of the literal.
        const channel_copy = try self.alloc.dupe(u8, channel);
        errdefer self.alloc.free(channel_copy);
        const payload_copy = try self.alloc.dupe(u8, payload);
        return Message{
            .channel = channel_copy,
            .payload = payload_copy,
        };
    }
}

/// UNSUBSCRIBE from all channels. Best-effort — connection is about to close.
pub fn unsubscribe(self: *Self, channel: []const u8) void {
    self.sendCommand(&.{ S_UNSUBSCRIBE, channel }) catch return;
}

/// Send SUBSCRIBE without reading the ack — for owners whose dedicated reader
/// thread consumes every inbound frame (`nextMessage` skips the ack via its
/// message-kind filter). Synchronous-ack `subscribe()` would race that reader.
pub fn sendSubscribe(self: *Self, channel: []const u8) !void {
    try self.sendCommand(&.{ S_SUBSCRIBE, channel });
}

/// UNSUBSCRIBE counterpart of `sendSubscribe` — ack consumed by the reader.
pub fn sendUnsubscribe(self: *Self, channel: []const u8) !void {
    try self.sendCommand(&.{ S_UNSUBSCRIBE, channel });
}

/// Install the configured read timeout now. `subscribe()` installs it after
/// its ack; owners that never call `subscribe()` (reader-thread consumers
/// using `sendSubscribe`) call this once after connect.
pub fn installReadTimeout(self: *Self) void {
    if (self.read_timeout_ms) |ms| self.transport.setReadTimeout(ms);
}

/// Hand this connection's socket to a caller-owned control block, so the
/// owner can bound its blocking sends without ever seeing the descriptor.
/// Returns false when `generation` was already retired.
pub fn attachTo(self: *Self, owner: *call_deadline.SocketOwner, generation: u64) bool {
    return owner.attachSocket(generation, self.transport.socketHandle());
}

fn sendCommand(self: *Self, argv: []const []const u8) !void {
    const writer = self.transport.writer();
    try writer.print("*{d}\r\n", .{argv.len});
    for (argv) |arg| {
        try writer.print("${d}\r\n", .{arg.len});
        try writer.writeAll(arg);
        try writer.writeAll("\r\n");
    }
    try writer.flush();
    // For TLS, `transport.writer()` is the TLS encryption layer; the
    // underlying TCP buffer is `transport.tls.stream_writer.interface`.
    // After `writer.flush()` pushes ciphertext into the TCP buffer, flush
    // THAT to get the bytes on the wire. No pre-flush — the TCP buffer has
    // nothing to flush before the TLS layer writes anything new. (Mirrors
    // the same two-layer sequence in redis_connection.zig writeArgvToTransport.)
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
const common = @import("common");
const logging = @import("log");
const call_deadline = @import("call_deadline");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
const redis_types = @import("redis_types.zig");
const log = logging.scoped(.redis_subscriber);

const EnvMap = common.env.Map;
