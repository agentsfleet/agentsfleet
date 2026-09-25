//! HTTP connection-pool pinning shared by the daemon and runner build graphs.
//! Direct TLS connects must prime the certificate bundle and validation clock
//! that `std.http.Client.fetch` otherwise initializes lazily.
//!
//! PRECONDITION — the caller must own `client` exclusively for the pin→fetch
//! window. Pinning connects, then RELEASES the connection back to the pool so
//! the following `fetch` pops the same one; a second thread fetching on the
//! same client could pop that connection first, and the watchdog would then
//! shut down a socket belonging to the other request while its own fetch ran
//! unbounded. Priming is likewise a bare `ca_bundle.rescan` — std's own path
//! scans into a local bundle and swaps it in under `ca_bundle_lock`, so a
//! concurrent handshake here could read a half-rebuilt trust store.
//!
//! Every call site satisfies this today: the daemon's connector and broker each
//! build a fresh `std.http.Client` per call (`bounded_fetch.fetch`,
//! `serve_broker.HttpClientExchange`), and the runner's persistent client is
//! per-worker-thread and driven serially. A future caller that pools or shares
//! a client across threads MUST take `ca_bundle_lock` and pin under its own
//! exclusion, or it silently reintroduces both races.

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

/// Why a pin could not be taken. Named so the refusal says WHICH step failed —
/// a bad address, an unreadable certificate bundle, or the DNS / TCP / TLS
/// step of the connect itself — instead of collapsing all of them into one
/// "pin failed" that an operator cannot act on.
pub const PinError = error{
    /// An empty host string reached the connect.
    EmptyHost,
    /// TLS was asked for but the certificate bundle could not be loaded.
    CertificateBundleUnavailable,
} || std.Io.net.HostName.ValidateError || std.http.Client.ConnectError;

/// Prime-then-pin for callers that hold host/port/tls (the runner's persistent
/// control-plane client). Connects the pooled handle the next `fetch` will pop
/// and returns its socket for a deadline watchdog; an error means certificate
/// state or the connect is unusable and the caller must not arm-and-fetch on
/// an assumed pin. The connect's own error passes through untouched: it is
/// what tells a DNS miss from a refused port from a failed handshake.
pub fn connectPinned(client: *std.http.Client, host_str: []const u8, port: u16, tls: bool) PinError!std.Io.net.Socket.Handle {
    if (host_str.len == 0) return error.EmptyHost;
    primeTlsForDirectConnect(client, client.io, tls);
    if (tls and client.now == null) return error.CertificateBundleUnavailable;
    const host = try std.Io.net.HostName.init(host_str);
    const conn = try client.connect(host, port, if (tls) .tls else .plain);
    const handle = conn.stream_writer.stream.socket.handle;
    client.connection_pool.release(conn, client.io);
    return handle;
}

test {
    _ = @import("http_pin_test.zig");
}
