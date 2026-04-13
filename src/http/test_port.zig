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
const posix = std.posix;

pub fn allocFreePort() !u16 {
    const sock = try posix.socket(posix.AF.INET, posix.SOCK.STREAM, posix.IPPROTO.TCP);
    defer posix.close(sock);

    try posix.setsockopt(
        sock,
        posix.SOL.SOCKET,
        posix.SO.REUSEADDR,
        &std.mem.toBytes(@as(c_int, 1)),
    );

    const addr = try std.net.Address.parseIp4("127.0.0.1", 0);
    try posix.bind(sock, &addr.any, addr.getOsSockLen());

    var bound: std.net.Address = undefined;
    var len: posix.socklen_t = @sizeOf(std.net.Address);
    try posix.getsockname(sock, &bound.any, &len);
    return bound.getPort();
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
    try std.testing.expect(p >= 1024);
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
    // The load-bearing claim of the helper: after getsockname + close, the
    // caller can bind the port without AddressInUse. If this fails, the
    // whole test-infra fix is broken.
    const port = try allocFreePort();

    const sock = try posix.socket(posix.AF.INET, posix.SOCK.STREAM, posix.IPPROTO.TCP);
    defer posix.close(sock);
    try posix.setsockopt(
        sock,
        posix.SOL.SOCKET,
        posix.SO.REUSEADDR,
        &std.mem.toBytes(@as(c_int, 1)),
    );
    const addr = try std.net.Address.parseIp4("127.0.0.1", port);
    try posix.bind(sock, &addr.any, addr.getOsSockLen());
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
