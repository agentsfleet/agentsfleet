//! Test-only helper: ask the kernel for a free TCP port on 127.0.0.1.
//!
//! We bind a throwaway socket to port 0, let the kernel assign an unused
//! port, read it back with getsockname, then close. The caller passes that
//! port to httpz. httpz sets SO_REUSEADDR, so TIME_WAIT on our close doesn't
//! block its bind.
//!
//! There is a sub-millisecond TOCTOU window between our close and httpz's
//! bind where another process could steal the port. On a quiet test runner
//! this has never been observed. If it ever does become a real flake, the
//! next step is to hand httpz the already-bound fd instead of closing.

const std = @import("std");
const common = @import("common");

const FIRST_UNPRIVILEGED_PORT = 1024;

const net = std.Io.net;

/// A bound, listening loopback socket plus its kernel-assigned port. The server
/// stays open so the caller can `accept`; the caller owns `deinit`.
pub const Loopback = struct {
    server: net.Server,
    port: u16,
};

/// Bind a loopback TCP listener on an ephemeral port (0 → kernel-assigned) and
/// report that port. Zig 0.16 removed raw `std.posix` socket/bind/getsockname;
/// the listener goes through `std.Io.net` and the port is read off the raw
/// handle via libc. Shared single home for the getsockname incantation so test
/// socket fakes (redis, etc.) and `allocFreePort` don't each re-roll it.
pub fn listenLoopback(io: std.Io) !Loopback {
    var addr = try net.IpAddress.parseIp4("127.0.0.1", 0);
    var server = try addr.listen(io, .{ .reuse_address = true });
    errdefer server.deinit(io);
    return .{ .server = server, .port = try boundPort(server.socket.handle) };
}

pub fn allocFreePort() !u16 {
    const io = common.globalIo();
    // Bind a throwaway listener, read its port, then close. SO_REUSEADDR (set
    // here and by httpz) keeps TIME_WAIT on our close from blocking httpz's
    // bind to the same port across the sub-ms TOCTOU window.
    var lp = try listenLoopback(io);
    lp.server.deinit(io);
    return lp.port;
}

/// Read the kernel-assigned local port off a bound socket handle. `std.Io.net`
/// exposes no getsockname, so go through libc on the raw fd.
fn boundPort(handle: net.Socket.Handle) !u16 {
    // SAFETY: written by getsockname via @ptrCast(&sa) below before sa.port is read.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

test "allocFreePort returns a non-zero port" {
    const p = try allocFreePort();
    try std.testing.expect(p != 0);
}

test "allocFreePort returns a port in the valid unprivileged range" {
    // Ephemeral ports on Linux default to 32768-60999 and on Darwin to
    // 49152-65535. We don't pin to a kernel-specific range, just assert
    // the port is unprivileged (>=1024) and within u16.
    const p = try allocFreePort();
    try std.testing.expect(p >= FIRST_UNPRIVILEGED_PORT);
}

test "allocFreePort: two consecutive calls both return valid ports" {
    // Distinctness isn't an invariant — the Linux kernel legitimately reuses
    // freshly-released ephemeral ports across short windows. What matters is
    // that each call succeeds and returns a plausible port; bindability is
    // covered by a dedicated test below.
    const a = try allocFreePort();
    const b = try allocFreePort();
    try std.testing.expect(a != 0);
    try std.testing.expect(b != 0);
}

test "allocFreePort: returned port is immediately bindable by the caller" {
    // The load-bearing claim of the helper: after listenLoopback + close, the
    // caller can re-bind the port without AddressInUse. If this fails, the
    // whole test-infra fix is broken. SO_REUSEADDR (set by listen) covers the
    // TIME_WAIT on the freed port.
    const io = common.globalIo();
    const port = try allocFreePort();
    var addr = try net.IpAddress.parseIp4("127.0.0.1", port);
    var server = try addr.listen(io, .{ .reuse_address = true });
    server.deinit(io);
}

test "allocFreePort: 64 sequential allocations all succeed" {
    // Regression for the original flake: fixed-counter port allocation
    // panicked on CI when the kernel had already grabbed the port. This
    // asserts allocFreePort scales to a full test suite's churn without
    // ever failing. Port values may repeat — the kernel legitimately reuses
    // freshly-released ephemeral ports — and that's fine because httpz sets
    // SO_REUSEADDR.
    var i: usize = 0;
    while (i < 64) : (i += 1) {
        const p = try allocFreePort();
        try std.testing.expect(p != 0);
    }
}
