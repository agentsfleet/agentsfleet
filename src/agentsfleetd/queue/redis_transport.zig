const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const logging = @import("log");
const redis_config = @import("redis_config.zig");
const error_codes = @import("../errors/error_registry.zig");
const tripwire = @import("tripwire");
const call_deadline = @import("call_deadline");

/// Zig 0.16 moved sockets under `std.Io.net` (io-threaded Stream/Reader/Writer).
const net = std.Io.net;

const log = logging.scoped(.redis_queue);

/// Fail points for `PlainTransport.init`'s buffer-alloc ladder (rule A6). Armed
/// only in tests; comptime-erased in production. The loop-all-failpoints test
/// lives in `redis_test.zig`.
pub const plain_init_tw = tripwire.module(enum { read_buffer, write_buffer }, error{OutOfMemory});

/// Best-effort TCP keepalive so an idle Upstash connection is detected by the
/// kernel within ~60s instead of sitting silently dead until the next request.
/// Failures are logged at debug and swallowed — keepalive is a hardening, not
/// a correctness guarantee; reconnect-on-error is the actual safety net.
/// Applied automatically from each transport's init.
const S_TRANSPORT_CONNECTED = "transport_connected";

fn applyKeepalive(stream: net.Stream) void {
    const sock = stream.socket.handle;
    const enable: c_int = 1;
    std.posix.setsockopt(sock, std.posix.SOL.SOCKET, std.posix.SO.KEEPALIVE, std.mem.asBytes(&enable)) catch |err| {
        log.debug("keepalive_enable_failed", .{ .err = @errorName(err) });
        return;
    };

    const idle: c_int = 30;
    const intvl: c_int = 10;
    const cnt: c_int = 3;

    switch (builtin.target.os.tag) {
        .linux => {
            const TCP_KEEPIDLE: u32 = 4;
            const TCP_KEEPINTVL: u32 = 5;
            const TCP_KEEPCNT: u32 = 6;
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPIDLE, std.mem.asBytes(&idle)) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPINTVL, std.mem.asBytes(&intvl)) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPCNT, std.mem.asBytes(&cnt)) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
        },
        .macos, .ios, .tvos, .watchos => {
            const TCP_KEEPALIVE: u32 = 0x10;
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPALIVE, std.mem.asBytes(&idle)) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
        },
        else => {},
    }
}

/// Socket reader that bounds each read by an optional millisecond timeout.
///
/// Zig 0.16's `std.Io.net.Stream.Reader` routes reads through `Io.Threaded`,
/// which treats `EAGAIN` (what `SO_RCVTIMEO` produces on a timed-out read) as a
/// programmer bug and panics. So the redis transport reads the fd directly: a
/// blocking `poll(2)` provides the timeout (the socket stays blocking, so
/// writes are untouched), and a hit deadline sets `timed_out` so the
/// Connection can surface a distinct `RedisRequestTimeout` instead of an
/// opaque read failure. Only `stream` is implemented; the buffered
/// `std.Io.Reader` interface drives it via the default `readVec`.
const TimeoutReader = struct {
    interface: std.Io.Reader,
    fd: std.posix.fd_t,
    /// Per-read budget in milliseconds; null blocks until data or peer close.
    timeout_ms: ?u32 = null,
    /// Set when a read hit its deadline; inspected (and reset) by the
    /// Connection to tell a timeout apart from any other read failure.
    timed_out: bool = false,

    fn init(fd: std.posix.fd_t, buffer: []u8) TimeoutReader {
        return .{
            .interface = .{
                .vtable = &.{ .stream = streamRead },
                .buffer = buffer,
                .seek = 0,
                .end = 0,
            },
            .fd = fd,
        };
    }

    fn streamRead(io_r: *std.Io.Reader, io_w: *std.Io.Writer, limit: std.Io.Limit) std.Io.Reader.StreamError!usize {
        const self: *TimeoutReader = @alignCast(@fieldParentPtr("interface", io_r));
        self.timed_out = false;
        const dest = limit.slice(try io_w.writableSliceGreedy(1));
        if (self.timeout_ms) |ms| {
            var pfds = [_]std.posix.pollfd{.{ .fd = self.fd, .events = std.posix.POLL.IN, .revents = 0 }};
            const ready = std.posix.poll(&pfds, @intCast(ms)) catch return error.ReadFailed;
            if (ready == 0) {
                self.timed_out = true;
                return error.ReadFailed;
            }
        }
        const n = std.posix.read(self.fd, dest) catch return error.ReadFailed;
        if (n == 0) return error.EndOfStream;
        io_w.advance(n);
        return n;
    }
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const PlainTransport = struct {
    const Self = @This();

    stream: net.Stream,
    stream_reader: TimeoutReader,
    stream_writer: net.Stream.Writer,
    read_buffer: []u8,
    write_buffer: []u8,

    pub fn init(io: std.Io, alloc: std.mem.Allocator, stream: net.Stream) !PlainTransport {
        // Own the stream on entry — any subsequent failure must close it
        // so callers (incl. dialAndAuth on every reconnect) cannot leak fds.
        errdefer stream.close(io);
        applyKeepalive(stream);
        try plain_init_tw.check(.read_buffer);
        const read_buffer = try alloc.alloc(u8, 16 * 1024);
        errdefer alloc.free(read_buffer);
        try plain_init_tw.check(.write_buffer);
        const write_buffer = try alloc.alloc(u8, 16 * 1024);
        errdefer alloc.free(write_buffer);

        log.debug(S_TRANSPORT_CONNECTED, .{ .mode = "plain" });

        return .{
            .stream = stream,
            .stream_reader = TimeoutReader.init(stream.socket.handle, read_buffer),
            .stream_writer = stream.writer(io, write_buffer),
            .read_buffer = read_buffer,
            .write_buffer = write_buffer,
        };
    }

    pub fn deinit(self: *Self, io: std.Io, alloc: std.mem.Allocator) void {
        self.stream.close(io);
        alloc.free(self.read_buffer);
        alloc.free(self.write_buffer);
        self.* = undefined;
    }

    pub fn reader(self: *Self) *std.Io.Reader {
        return &self.stream_reader.interface;
    }

    pub fn writer(self: *Self) *std.Io.Writer {
        return &self.stream_writer.interface;
    }
};

const TlsTransport = struct {
    const Self = @This();

    stream: net.Stream,
    stream_reader: *TimeoutReader,
    stream_writer: *net.Stream.Writer,
    tls_client: std.crypto.tls.Client,
    socket_read_buffer: []u8,
    socket_write_buffer: []u8,
    tls_read_buffer: []u8,
    tls_write_buffer: []u8,
    ca_bundle: std.crypto.Certificate.Bundle,

    pub fn initInPlace(self: *Self, io: std.Io, alloc: std.mem.Allocator, stream: net.Stream, host: []const u8, ca_file_path: ?[]const u8) !void {
        // Own the stream on entry — any subsequent failure (CA load, buffer
        // alloc, TLS handshake) must close it so callers (incl. dialAndAuth
        // on every reconnect) cannot leak fds.
        errdefer stream.close(io);
        applyKeepalive(stream);
        // `REDIS_TLS_CA_CERT_FILE` is resolved once into the Config (env is a
        // Zig 0.16 snapshot); the dial path just borrows the resolved path.
        var ca_bundle = try redis_config.loadCaBundle(io, alloc, ca_file_path);
        errdefer ca_bundle.deinit(alloc);

        const socket_read_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(socket_read_buffer);
        const socket_write_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(socket_write_buffer);
        const tls_read_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(tls_read_buffer);
        const tls_write_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len * 8);
        errdefer alloc.free(tls_write_buffer);
        const stream_reader = try alloc.create(TimeoutReader);
        errdefer alloc.destroy(stream_reader);
        const stream_writer = try alloc.create(net.Stream.Writer);
        errdefer alloc.destroy(stream_writer);
        stream_reader.* = TimeoutReader.init(stream.socket.handle, socket_read_buffer);
        stream_writer.* = stream.writer(io, tls_write_buffer);

        self.* = .{
            .stream = stream,
            .stream_reader = stream_reader,
            .stream_writer = stream_writer,
            // SAFETY: written by surrounding init logic before any read of this storage.
            .tls_client = undefined,
            .socket_read_buffer = socket_read_buffer,
            .socket_write_buffer = socket_write_buffer,
            .tls_read_buffer = tls_read_buffer,
            .tls_write_buffer = tls_write_buffer,
            .ca_bundle = ca_bundle,
        };

        // Zig 0.16 tls.Client.init needs caller-supplied entropy + a wall clock,
        // and the CA bundle is verified under a lock. Entropy + lock are read
        // only during this synchronous handshake, so a stack buffer/lock is safe
        // (Options docs: entropy pointer is not captured).
        var entropy_buf: [std.crypto.tls.Client.Options.entropy_len]u8 = undefined;
        try common.secureRandomBytes(&entropy_buf);
        var ca_lock: std.Io.RwLock = .init;

        self.tls_client = std.crypto.tls.Client.init(
            &self.stream_reader.interface,
            &self.stream_writer.interface,
            .{
                .host = .{ .explicit = host },
                .ca = .{ .bundle = .{
                    .gpa = alloc,
                    .io = io,
                    .lock = &ca_lock,
                    .bundle = &self.ca_bundle,
                } },
                .read_buffer = self.tls_read_buffer,
                .write_buffer = self.socket_write_buffer,
                .entropy = &entropy_buf,
                .realtime_now = std.Io.Timestamp.now(io, .real),
                .allow_truncation_attacks = false,
            },
        ) catch |err| {
            log.err("tls_handshake_failed", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .host = host });
            return err;
        };
        log.debug(S_TRANSPORT_CONNECTED, .{ .mode = "tls", .host = host });
    }

    pub fn deinit(self: *Self, io: std.Io, alloc: std.mem.Allocator) void {
        self.stream.close(io);
        self.ca_bundle.deinit(alloc);
        alloc.destroy(self.stream_reader);
        alloc.destroy(self.stream_writer);
        alloc.free(self.socket_read_buffer);
        alloc.free(self.socket_write_buffer);
        alloc.free(self.tls_read_buffer);
        alloc.free(self.tls_write_buffer);
        self.* = undefined;
    }

    pub fn reader(self: *Self) *std.Io.Reader {
        return &self.tls_client.reader;
    }

    pub fn writer(self: *Self) *std.Io.Writer {
        return &self.tls_client.writer;
    }
};

pub const Transport = union(enum) {
    const Self = @This();

    plain: PlainTransport,
    tls: TlsTransport,

    pub fn deinit(self: *Self, io: std.Io, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .plain => |*p| p.deinit(io, alloc),
            .tls => |*t| t.deinit(io, alloc),
        }
        self.* = undefined;
    }

    pub fn reader(self: *Self) *std.Io.Reader {
        return switch (self.*) {
            .plain => |*p| p.reader(),
            .tls => |*t| t.reader(),
        };
    }

    pub fn writer(self: *Self) *std.Io.Writer {
        return switch (self.*) {
            .plain => |*p| p.writer(),
            .tls => |*t| t.writer(),
        };
    }

    /// Arm (or clear) the per-read timeout. Non-null `ms` bounds each read by
    /// a `poll(2)` deadline on the transport's `TimeoutReader`; on expiry the
    /// read fails and `readTimedOut()` returns true so the Connection surfaces
    /// a distinct `RedisRequestTimeout`. Null blocks until data or peer close.
    /// Replaces `SO_RCVTIMEO`, which panics under 0.16's threaded reader.
    pub fn setReadTimeout(self: *Self, ms: ?u32) void {
        switch (self.*) {
            .plain => |*p| p.stream_reader.timeout_ms = ms,
            .tls => |*t| t.stream_reader.timeout_ms = ms,
        }
    }

    /// True if the most recent read on this transport hit its timeout deadline
    /// (vs a peer drop or other read failure). Lets the Connection map an
    /// otherwise-opaque `ReadFailed` to a distinct `RedisRequestTimeout`.
    pub fn readTimedOut(self: *Self) bool {
        return switch (self.*) {
            .plain => |*p| p.stream_reader.timed_out,
            .tls => |*t| t.stream_reader.timed_out,
        };
    }

    /// Publish this connection's socket to a caller-owned control block, which
    /// is the ONLY way it leaves the transport. There is deliberately no
    /// descriptor accessor: a caller that could read the handle could arm a
    /// deadline on a number, and a recycled number names a different
    /// connection. The owner stores it against `generation` and re-validates
    /// under its own lock before any interrupt can touch it.
    /// Returns false when `generation` was already retired.
    pub fn attachTo(self: *Self, owner: *call_deadline.SocketOwner, generation: u64) bool {
        const handle: std.posix.fd_t = switch (self.*) {
            .plain => |*p| p.stream_reader.fd,
            .tls => |*t| t.stream_reader.fd,
        };
        return owner.attachSocket(generation, handle);
    }
};
