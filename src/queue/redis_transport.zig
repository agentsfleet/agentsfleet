const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");
const redis_config = @import("redis_config.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.redis_queue);

/// Best-effort TCP keepalive so an idle Upstash connection is detected by the
/// kernel within ~60s instead of sitting silently dead until the next request.
/// Failures are logged at debug and swallowed — keepalive is a hardening, not
/// a correctness guarantee; reconnect-on-error is the actual safety net.
/// Applied automatically from each transport's init.
fn applyKeepalive(stream: std.net.Stream) void {
    const sock = stream.handle;
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
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPIDLE, std.mem.asBytes(&idle)) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPINTVL, std.mem.asBytes(&intvl)) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPCNT, std.mem.asBytes(&cnt)) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
        },
        .macos, .ios, .tvos, .watchos => {
            const TCP_KEEPALIVE: u32 = 0x10;
            std.posix.setsockopt(sock, std.posix.IPPROTO.TCP, TCP_KEEPALIVE, std.mem.asBytes(&idle)) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
        },
        else => {},
    }
}

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const PlainTransport = struct {
    stream: std.net.Stream,
    stream_reader: std.net.Stream.Reader,
    stream_writer: std.net.Stream.Writer,
    read_buffer: []u8,
    write_buffer: []u8,

    pub fn init(alloc: std.mem.Allocator, stream: std.net.Stream) !PlainTransport {
        // Own the stream on entry — any subsequent failure must close it
        // so callers (incl. dialAndAuth on every reconnect) cannot leak fds.
        errdefer stream.close();
        applyKeepalive(stream);
        const read_buffer = try alloc.alloc(u8, 16 * 1024);
        errdefer alloc.free(read_buffer);
        const write_buffer = try alloc.alloc(u8, 16 * 1024);
        errdefer alloc.free(write_buffer);

        log.debug("transport_connected", .{ .mode = "plain" });

        return .{
            .stream = stream,
            .stream_reader = stream.reader(read_buffer),
            .stream_writer = stream.writer(write_buffer),
            .read_buffer = read_buffer,
            .write_buffer = write_buffer,
        };
    }

    pub fn deinit(self: *PlainTransport, alloc: std.mem.Allocator) void {
        self.stream.close();
        alloc.free(self.read_buffer);
        alloc.free(self.write_buffer);
    }

    pub fn reader(self: *PlainTransport) *std.Io.Reader {
        return self.stream_reader.interface();
    }

    pub fn writer(self: *PlainTransport) *std.Io.Writer {
        return &self.stream_writer.interface;
    }
};

const TlsTransport = struct {
    stream: std.net.Stream,
    stream_reader: *std.net.Stream.Reader,
    stream_writer: *std.net.Stream.Writer,
    tls_client: std.crypto.tls.Client,
    socket_read_buffer: []u8,
    socket_write_buffer: []u8,
    tls_read_buffer: []u8,
    tls_write_buffer: []u8,
    ca_bundle: std.crypto.Certificate.Bundle,

    pub fn initInPlace(self: *TlsTransport, alloc: std.mem.Allocator, stream: std.net.Stream, host: []const u8) !void {
        // Own the stream on entry — any subsequent failure (CA load, buffer
        // alloc, TLS handshake) must close it so callers (incl. dialAndAuth
        // on every reconnect) cannot leak fds.
        errdefer stream.close();
        applyKeepalive(stream);
        const ca_file = std.process.getEnvVarOwned(alloc, "REDIS_TLS_CA_CERT_FILE") catch null;
        defer if (ca_file) |v| alloc.free(v);
        var ca_bundle = try redis_config.loadCaBundle(alloc, ca_file);
        errdefer ca_bundle.deinit(alloc);

        const socket_read_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(socket_read_buffer);
        const socket_write_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(socket_write_buffer);
        const tls_read_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len);
        errdefer alloc.free(tls_read_buffer);
        const tls_write_buffer = try alloc.alloc(u8, std.crypto.tls.Client.min_buffer_len * 8);
        errdefer alloc.free(tls_write_buffer);
        const stream_reader = try alloc.create(std.net.Stream.Reader);
        errdefer alloc.destroy(stream_reader);
        const stream_writer = try alloc.create(std.net.Stream.Writer);
        errdefer alloc.destroy(stream_writer);
        stream_reader.* = stream.reader(socket_read_buffer);
        stream_writer.* = stream.writer(tls_write_buffer);

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

        self.tls_client = std.crypto.tls.Client.init(
            self.stream_reader.interface(),
            &self.stream_writer.interface,
            .{
                .host = .{ .explicit = host },
                .ca = .{ .bundle = self.ca_bundle },
                .read_buffer = self.tls_read_buffer,
                .write_buffer = self.socket_write_buffer,
                .allow_truncation_attacks = false,
            },
        ) catch |err| {
            log.err("tls_handshake_failed", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .host = host });
            return err;
        };
        log.debug("transport_connected", .{ .mode = "tls", .host = host });
    }

    pub fn deinit(self: *TlsTransport, alloc: std.mem.Allocator) void {
        self.stream.close();
        self.ca_bundle.deinit(alloc);
        alloc.destroy(self.stream_reader);
        alloc.destroy(self.stream_writer);
        alloc.free(self.socket_read_buffer);
        alloc.free(self.socket_write_buffer);
        alloc.free(self.tls_read_buffer);
        alloc.free(self.tls_write_buffer);
    }

    pub fn reader(self: *TlsTransport) *std.Io.Reader {
        return &self.tls_client.reader;
    }

    pub fn writer(self: *TlsTransport) *std.Io.Writer {
        return &self.tls_client.writer;
    }
};

pub const Transport = union(enum) {
    plain: PlainTransport,
    tls: TlsTransport,

    pub fn deinit(self: *Transport, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .plain => |*p| p.deinit(alloc),
            .tls => |*t| t.deinit(alloc),
        }
    }

    pub fn reader(self: *Transport) *std.Io.Reader {
        return switch (self.*) {
            .plain => |*p| p.reader(),
            .tls => |*t| t.reader(),
        };
    }

    pub fn writer(self: *Transport) *std.Io.Writer {
        return switch (self.*) {
            .plain => |*p| p.writer(),
            .tls => |*t| t.writer(),
        };
    }
};
