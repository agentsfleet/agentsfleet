//! One Redis connection — pooled, dedicated-blocking, or subscriber.
//!
//! Role is `const` after init (Invariant 7) — set via the role-specific
//! constructor and never mutated. State transitions go through
//! `transitionTo()` only (Invariant 14) and `.closed` is reachable exactly
//! once per Connection lifetime. After `close()`, `fd` is `INVALID_FD` and
//! every IO path asserts otherwise (Invariant 13).
//!
//! Slice 1 lays the shape down; slice 3 wires `redis_client.zig` onto it.

const Connection = @This();

pub const Role = enum { pooled, blocking_consumer, subscriber };

/// State machine kept private to slice 1: every transition goes through
/// `transitionTo()` inside this file, and Pool reads the field with
/// literal-typed comparisons (`.poisoned` / `.active`). Slice 3's retry
/// layer may need to inspect state cross-file — promote then.
const ConnectionState = enum {
    active,
    poisoned,
    closing,
    closed,
};

/// `-1` is the POSIX "no fd" sentinel. After `deinit()` closes the socket,
/// `self.fd` is overwritten with this so any subsequent IO assertion fires
/// in debug builds — see Invariant 13. Tests in `redis_connection_test.zig`
/// observe it directly to verify the close path.
pub const INVALID_FD: std.posix.fd_t = -1;

/// Slice 1 carries a private local error set sufficient for `command`'s
/// return signature. Slice 2 supplants this with the typed `RedisError`
/// set in `redis_errors.zig` (per spec §4 — `isResumable` ships there
/// too); callers use `try` on `command()` and never name this type.
const Error = error{
    BrokenPipe,
    ConnectionResetByPeer,
    ReadFailed,
    WriteFailed,
    RedisCommandError,
};

// === Fields ===

role: Role,
state: ConnectionState,
fd: std.posix.fd_t,
transport: redis_transport.Transport,
alloc: std.mem.Allocator,
/// Borrowed from the owning Pool (pooled role) or the spawning thread
/// (dedicated / subscriber roles). Caller guarantees lifetime ≥ Connection's.
cfg: *const redis_config.Config,
/// Embedded link node — only attached when `role == .pooled` and the
/// connection sits on Pool.idle. Inert for other roles.
node: std.SinglyLinkedList.Node = .{},

// === Constructor ===

/// Dial Redis and authenticate, returning a Connection in the requested role.
/// `role` is `const` for the connection's lifetime — boundary code
/// (`Pool.release`, `Subscriber.connect`) asserts it; nothing in this file
/// mutates `self.role` after init.
pub fn init(alloc: std.mem.Allocator, cfg: *const redis_config.Config, role: Role) !Connection {
    var transport = try dialAndAuth(alloc, cfg.*);
    errdefer transport.deinit(alloc);
    return .{
        .role = role,
        .state = .active,
        .fd = transportFd(&transport),
        .transport = transport,
        .alloc = alloc,
        .cfg = cfg,
    };
}

// === Lifecycle ===

pub fn deinit(self: *Connection) void {
    // .active or .poisoned → .closing → .closed
    // .closing → .closed (already requested teardown)
    // .closed → asserts (Invariant 14: terminal, reachable once)
    switch (self.state) {
        .active, .poisoned => self.transitionTo(.closing),
        .closing => {},
        .closed => std.debug.assert(false),
    }
    self.transport.deinit(self.alloc);
    self.fd = INVALID_FD;
    self.transitionTo(.closed);
}

fn transitionTo(self: *Connection, new_state: ConnectionState) void {
    const legal = switch (self.state) {
        .active => new_state == .poisoned or new_state == .closing,
        .poisoned => new_state == .closing,
        .closing => new_state == .closed,
        .closed => false,
    };
    std.debug.assert(legal);
    self.state = new_state;
}

// === Command ===

/// Send one RESP request, read exactly one RESP reply. `.err` replies
/// surface as `error.RedisCommandError` — callers that need to inspect
/// the server's error message use `commandAllowError` and switch on the
/// returned `RespValue`.
///
/// PIPELINING FORBIDDEN — see Invariants 12 in the file header. One argv
/// in, one reply out. Any IO error transitions the connection to
/// `.poisoned`; the caller (Pool.release / Client retry layer) closes
/// from there.
pub fn command(self: *Connection, argv: []const []const u8) Error!redis_protocol.RespValue {
    var value = try self.commandAllowError(argv);
    if (value == .err) {
        // Resumable: the connection stayed in protocol sync; caller
        // releases ok=true so the same conn serves the next request.
        // Per-error logging of `value.err` lands with the broader error
        // surfacing pass — slice that lands typed XADD/XACK variants.
        value.deinit(self.alloc);
        return error.RedisCommandError;
    }
    return value;
}

/// Like `command` but returns the raw `RespValue` even when the reply is
/// `.err`. Used by callers that need the server's error message verbatim
/// (e.g. SET NX returning nil on existing key, XGROUP CREATE returning
/// BUSYGROUP on already-created group). IO errors still transition the
/// connection to `.poisoned` and surface as transport-level errors.
pub fn commandAllowError(self: *Connection, argv: []const []const u8) Error!redis_protocol.RespValue {
    std.debug.assert(self.fd != INVALID_FD);
    std.debug.assert(self.state == .active);

    writeArgvToTransport(&self.transport, argv) catch |err| {
        self.transitionTo(.poisoned);
        return mapWriteError(err);
    };
    return redis_protocol.readRespValue(self.alloc, self.transport.reader()) catch |err| {
        self.transitionTo(.poisoned);
        return mapReadError(err);
    };
}

fn mapWriteError(err: anyerror) Error {
    return switch (err) {
        error.BrokenPipe => error.BrokenPipe,
        error.ConnectionResetByPeer => error.ConnectionResetByPeer,
        else => error.WriteFailed,
    };
}

fn mapReadError(err: anyerror) Error {
    // SO_RCVTIMEO surfaces as `error.ReadFailed` at the std.Io.Reader layer
    // (ZIG_RULES.md "TLS Transport"); slice 5 wires `RedisRequestTimeout`
    // around that translation when the request-path timeout is set.
    return switch (err) {
        error.BrokenPipe => error.BrokenPipe,
        error.ConnectionResetByPeer => error.ConnectionResetByPeer,
        else => error.ReadFailed,
    };
}

// === Helpers ===

fn transportFd(transport: *redis_transport.Transport) std.posix.fd_t {
    return switch (transport.*) {
        .plain => |*p| p.stream.handle,
        .tls => |*t| t.stream.handle,
    };
}

fn dialAndAuth(alloc: std.mem.Allocator, cfg: redis_config.Config) !redis_transport.Transport {
    const stream = try std.net.tcpConnectToHost(alloc, cfg.host, cfg.port);

    var transport: redis_transport.Transport = undefined;
    if (cfg.use_tls) {
        transport = .{ .tls = undefined };
        try transport.tls.initInPlace(alloc, stream, cfg.host);
    } else {
        transport = .{ .plain = try redis_transport.PlainTransport.init(alloc, stream) };
    }
    errdefer transport.deinit(alloc);

    if (cfg.password) |pwd| {
        const argv: []const []const u8 = if (cfg.username) |usr|
            &.{ "AUTH", usr, pwd }
        else
            &.{ "AUTH", pwd };
        try writeArgvToTransport(&transport, argv);
        var resp = try redis_protocol.readRespValue(alloc, transport.reader());
        defer resp.deinit(alloc);
        try redis_protocol.ensureSimpleOk(resp);
    }
    return transport;
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

// === Imports ===

const std = @import("std");
const redis_config = @import("redis_config.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_transport = @import("redis_transport.zig");
