//! Scripted one-shot control plane for the operator-command tests.
//!
//! Accepts a single connection and answers every request on it with one
//! configured status line + body — the smallest thing that lets `doctor` and
//! `status` pin their operator-facing verdicts against real HTTP instead of
//! assuming the client's error mapping. Mirrors the keep-alive stub in
//! `daemon/control_plane_client_test.zig`; carried as a `_test` file so kcov
//! excludes it from the product denominator.

const std = @import("std");

pub const StubStatus = struct { line: []const u8, body: []const u8 };

pub const OneShotPlane = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    status: StubStatus,

    pub fn serve(self: *OneShotPlane) void {
        const conn = self.listener.accept(self.io) catch return;
        defer conn.close(self.io);
        var rbuf: [4096]u8 = undefined;
        while (true) {
            var total: usize = 0;
            while (std.mem.indexOf(u8, rbuf[0..total], "\r\n\r\n") == null) {
                const n = std.posix.read(conn.socket.handle, rbuf[total..]) catch return;
                if (n == 0) return;
                total += n;
                if (total == rbuf.len) return;
            }
            var wbuf: [512]u8 = undefined;
            var w = conn.writer(self.io, &wbuf);
            w.interface.print(
                "HTTP/1.1 {s}\r\nContent-Type: application/json\r\nContent-Length: {d}\r\n\r\n{s}",
                .{ self.status.line, self.status.body.len, self.status.body },
            ) catch return;
            w.interface.flush() catch return;
        }
    }
};

/// Local port of a listener bound to port 0. Mirrors the daemon test helper.
pub fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the !=0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// Swap fd 1 for a throwaway pipe while a test drives code that writes to the
/// process's real stdout. Under `zig build test` stdout IS the build-runner's
/// protocol pipe — a stray byte desyncs it and the whole lane hangs, which is
/// exactly what happened the first time these CLI paths ran in-process.
pub const MutedStdout = struct {
    saved: std.c.fd_t,
    sink_read: std.c.fd_t,
    sink_write: std.c.fd_t,

    pub fn mute() !MutedStdout {
        // std.c, not std.posix — Zig 0.16 removed the posix wrappers for
        // dup/dup2/pipe (pipe_proto.zig routes the same way).
        const saved = std.c.dup(1);
        if (saved < 0) return error.DupFailed;
        var fds: [2]std.c.fd_t = undefined;
        if (std.c.pipe(&fds) != 0) return error.PipeFailed;
        if (std.c.dup2(fds[1], 1) < 0) return error.Dup2Failed;
        return .{ .saved = saved, .sink_read = fds[0], .sink_write = fds[1] };
    }

    pub fn restore(self: *MutedStdout) void {
        _ = std.c.dup2(self.saved, 1);
        _ = std.c.close(self.saved);
        _ = std.c.close(self.sink_read);
        _ = std.c.close(self.sink_write);
        // SAFETY: every fd is closed above; poisoning traps a second restore.
        self.* = undefined;
    }
};
