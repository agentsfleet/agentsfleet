//! HTTP connection-pool pinning shared by the daemon and runner build graphs.
//! Direct TLS connects must prime the certificate bundle and validation clock
//! that `std.http.Client.fetch` otherwise initializes lazily.

const std = @import("std");

/// Populate the certificate bundle before a direct TLS connect, or refresh the
/// validation clock when the client is already primed. A failed first rescan
/// leaves `client.now` null; callers must refuse the direct TLS connect.
pub fn primeTlsForDirectConnect(client: *std.http.Client, io: std.Io, tls: bool) void {
    if (!tls) return;
    if (client.now != null) {
        client.now = std.Io.Clock.real.now(io);
        return;
    }
    const now = std.Io.Clock.real.now(io);
    client.ca_bundle.rescan(client.allocator, io, now) catch return;
    client.now = now;
}

/// Pin the pooled connection the next `fetch` will use and return its socket
/// handle for a deadline watchdog. Null means the URL, certificate state, or
/// connection is unusable, so the caller must refuse the armed fetch.
pub fn pinPooledHandle(client: *std.http.Client, url: []const u8) ?std.Io.net.Socket.Handle {
    const uri = std.Uri.parse(url) catch return null;
    const tls = std.ascii.eqlIgnoreCase(uri.scheme, "https");
    const port: u16 = uri.port orelse @as(u16, if (tls) 443 else 80);
    const raw_host = uri.host orelse return null;
    const host_str = switch (raw_host) {
        .raw => |r| r,
        .percent_encoded => |p| p,
    };
    if (host_str.len == 0) return null;
    primeTlsForDirectConnect(client, client.io, tls);
    if (tls and client.now == null) return null;
    const host = std.Io.net.HostName.init(host_str) catch return null;
    const conn = client.connect(host, port, if (tls) .tls else .plain) catch return null;
    const handle = conn.stream_writer.stream.socket.handle;
    client.connection_pool.release(conn, client.io);
    return handle;
}
