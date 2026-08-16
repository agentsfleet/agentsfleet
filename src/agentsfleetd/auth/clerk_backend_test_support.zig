//! Loopback HTTP server standing in for Clerk in `clerk_backend.zig`'s tests.
//!
//! The fetch paths under proof are private to that module, so their tests have
//! to live inside it — but the server they drive is ordinary test support and is
//! not shipped code. Held here, its naming form takes it out of the coverage
//! denominator while the in-file tests keep reaching it; inline in the product
//! file it read as permanently dark shipped lines.

const std = @import("std");

const REQUEST_BUF_LEN = 2048;
const RESPONSE: []const u8 = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";

/// Stand-in principal for the metadata writeback tests. Not a credential: the
/// loopback server answers 200 to anything, so the value only has to be
/// non-blank to clear the module's own secret check.
pub const SECRET = "sk_test_loopback_not_a_credential";
pub const API_BASE = "http://127.0.0.1:9";
pub const USER_ID = "user_metadata_writeback";
pub const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-aa0000000009";
pub const SCOPES = "fleet:admin credential:write";

/// The port the kernel actually chose for a listener bound to port 0.
pub fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// Accepts exactly one connection and answers 200. One-shot by design: each
/// test drives a single request and joins the thread, so a server that outlived
/// its test would be a leak rather than a convenience.
pub const OkServer = struct {
    pub fn run(listener: *std.Io.net.Server, io: std.Io) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [REQUEST_BUF_LEN]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        var sent: usize = 0;
        while (sent < RESPONSE.len) {
            const rc = std.posix.system.write(conn.socket.handle, RESPONSE[sent..].ptr, RESPONSE.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return;
            sent += @intCast(rc);
        }
    }
};
