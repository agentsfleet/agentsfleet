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

test "allocFreePort returns distinct ports on repeated calls" {
    const a = try allocFreePort();
    const b = try allocFreePort();
    // Not strictly guaranteed by POSIX, but Linux/Darwin reliably hand out
    // different ephemeral ports for consecutive binds.
    try std.testing.expect(a != b);
}
