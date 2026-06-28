//! pipe_proto.zig — typed length-prefixed framing over the lease pipes.
//!
//! Both lease pipes carry framed messages, multiplexed by `FrameType`:
//!   * child stdout (child→parent): zero-or-more `activity` frames streamed
//!     during execution (live-tail progress) interleaved with the occasional
//!     `credential_request` (an on-demand mint ask, M102 §3), then exactly one
//!     terminal `result` frame. The parent reads frames in order, forwards each
//!     `activity` frame to the control plane, services each `credential_request`
//!     inline, and parses the `result` frame as the `ExecutionResult`.
//!   * child stdin (parent→child): exactly one `lease` frame at startup (the
//!     work + inline secrets), then zero-or-more `credential_response` frames —
//!     one per `credential_request` the child raised. stdin stays open for the
//!     lease lifetime so the mint round-trip rides it (no extra descriptor, no
//!     new sandbox hole — the same stdin/stdout pair the memory channel uses).
//!
//! The child is single-threaded during a turn, so a `credential_request` is only
//! ever emitted between other frames (never concurrently): it writes the request,
//! blocks reading its `credential_response`, then resumes — no interleave race.
//! stdin/stdout are the two fds that cross the bwrap boundary cleanly, so both
//! channels ride them rather than a fragile extra descriptor.
//!
//! Frame = [1 byte type][4 byte big-endian length][payload]. The payload is
//! opaque to this module (the writer serializes, the reader hands bytes back);
//! framing owns only the envelope. Reads are bounded by the lease wall-clock
//! deadline so a stuck child cannot block the parent past `lease_expires_at`.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const globalIo = common.globalIo;

/// Header byte selecting the message class. Values are ASCII so a stray frame is
/// legible in a hexdump; the enum is the single source (RULE UFS). The first four
/// ride child→parent on stdout; `lease` + `credential_response` ride parent→child
/// on stdin; `credential_request` rides child→parent on stdout (M102 §3).
pub const FrameType = enum(u8) {
    activity = 'A',
    result = 'R',
    memory = 'M',
    usage = 'U',
    /// parent→child: the work + inline secrets, the first (and only) stdin frame
    /// at startup. Replaces the pre-§3 write-then-EOF lease feed so stdin can stay
    /// open as the credential-response channel for the lease lifetime.
    lease = 'L',
    /// child→parent: an on-demand mint ask (`PipeRequest`); the child blocks for
    /// its `credential_response` before resuming.
    credential_request = 'C',
    /// parent→child: the mint reply (`PipeResponse`) — a short-lived token or a
    /// typed rejection. Secret (VLT): never logged, only framed back to the child.
    credential_response = 'T',
};

const HEADER_LEN = 1 + 4; // type byte + u32 big-endian length

/// Child exit codes the parent's `classify` reads off the reaped `Term` when the
/// child aborts before a `result` frame (single source, RULE UFS). Clean codes,
/// not signals, so they survive bwrap's signal->exit translation.
///   SANDBOX_FAIL_EXIT      — fail-closed sandbox setup (Invariant 7) -> startup_posture.
///   SECCOMP_VIOLATION_EXIT — a denylisted syscall trapped -> landlock_deny.
///   GENERIC_FAIL_EXIT      — any other pre-result abort -> crash.
pub const SANDBOX_FAIL_EXIT: u8 = 78;
pub const SECCOMP_VIOLATION_EXIT: u8 = 79;
pub const GENERIC_FAIL_EXIT: u8 = 1;

/// Create an anonymous pipe via libc (`std.posix.pipe` was removed in Zig 0.16;
/// the runner links -lc). Returns `[read_fd, write_fd]`.
pub fn testOsPipe() error{PipeFailed}![2]std.posix.fd_t {
    var fds: [2]std.posix.fd_t = undefined;
    if (std.c.pipe(&fds) != 0) return error.PipeFailed;
    return fds;
}

/// One decoded frame. `payload` is owned by the caller's allocator.
pub const Frame = struct {
    ftype: FrameType,
    payload: []u8,
};

/// Outcome of one `readFrame` call. `eof` is clean (the child closed stdout at a
/// frame boundary — expected after the terminal result); `timed_out` means the
/// lease deadline elapsed mid-read.
pub const ReadOutcome = union(enum) {
    frame: Frame,
    eof,
    timed_out,
};

/// Cumulative token-usage snapshot riding a `usage` frame. Lifted to its own
/// file-as-struct (`UsageSnapshot.zig`) so the wire type owns its encode/decode/
/// fold + drift guard; re-exported here as the canonical `pipe_proto.UsageSnapshot`
/// the supervisor/renew path consumes.
pub const UsageSnapshot = @import("UsageSnapshot.zig");

/// Write one framed message to `fd`. Caller owns `payload`; it is copied to the
/// kernel here, not retained.
pub fn writeFrame(fd: std.posix.fd_t, ftype: FrameType, payload: []const u8) !void {
    var header: [HEADER_LEN]u8 = undefined;
    header[0] = @intFromEnum(ftype);
    std.mem.writeInt(u32, header[1..5], std.math.cast(u32, payload.len) orelse return error.FrameTooLarge, .big);
    try writeAll(fd, &header);
    try writeAll(fd, payload);
}

/// Read one framed message from `fd`, bounded by `deadline_ms` (absolute epoch
/// ms). Returns `.eof` at a clean frame boundary, `.timed_out` if the deadline
/// elapsed mid-frame, or `.frame` with an alloc-owned payload. `max_payload`
/// caps a single frame (defence against a runaway child).
pub fn readFrame(
    alloc: std.mem.Allocator,
    fd: std.posix.fd_t,
    deadline_ms: i64,
    max_payload: usize,
) !ReadOutcome {
    var header: [HEADER_LEN]u8 = undefined;
    switch (try readExact(fd, &header, deadline_ms)) {
        .timed_out => return .timed_out,
        .eof => |filled| return if (filled == 0) .eof else error.TruncatedFrame,
        .full => {},
    }

    const ftype = std.enums.fromInt(FrameType, header[0]) orelse return error.UnknownFrameType;
    const len: usize = std.mem.readInt(u32, header[1..5], .big);
    if (len > max_payload) return error.FrameTooLarge;

    const payload = try alloc.alloc(u8, len);
    errdefer alloc.free(payload);
    switch (try readExact(fd, payload, deadline_ms)) {
        .timed_out => {
            alloc.free(payload);
            return .timed_out;
        },
        .eof => return error.TruncatedFrame,
        .full => {},
    }
    return .{ .frame = .{ .ftype = ftype, .payload = payload } };
}

/// Whether `fd` became readable before the deadline. `.readable` includes a
/// closed write end (a subsequent read returns 0 = EOF).
pub const ReadyState = enum { readable, timed_out };

/// Wait until `fd` has data (or EOF) to read, or `deadline_ms` (absolute epoch
/// ms) passes. The supervisor uses this to wake at a renewal-tick cadence in the
/// idle gap BETWEEN frames: a tick must never interrupt a frame mid-read (that
/// would consume and discard partial bytes, desyncing the stream), so the frame
/// read itself always runs at the full lease deadline once data is present.
pub fn waitReadable(fd: std.posix.fd_t, deadline_ms: i64) !ReadyState {
    const remaining = deadline_ms - clock.nowMillis();
    if (remaining <= 0) return .timed_out;
    var fds = [_]std.posix.pollfd{.{ .fd = fd, .events = std.posix.POLL.IN, .revents = 0 }};
    const ready = try std.posix.poll(&fds, @intCast(@min(remaining, std.math.maxInt(i32))));
    return if (ready == 0) .timed_out else .readable;
}

/// Fill `buf` exactly, polling under the deadline. `.eof` carries how many bytes
/// arrived before EOF (0 = clean boundary); `.full` means `buf` is filled.
const FillState = union(enum) { full, eof: usize, timed_out };

fn readExact(fd: std.posix.fd_t, buf: []u8, deadline_ms: i64) !FillState {
    var off: usize = 0;
    while (off < buf.len) {
        const remaining = deadline_ms - clock.nowMillis();
        if (remaining <= 0) return .timed_out;
        var fds = [_]std.posix.pollfd{.{ .fd = fd, .events = std.posix.POLL.IN, .revents = 0 }};
        const ready = try std.posix.poll(&fds, @intCast(@min(remaining, std.math.maxInt(i32))));
        if (ready == 0) return .timed_out;
        const n = try std.posix.read(fd, buf[off..]);
        if (n == 0) return .{ .eof = off };
        off += n;
    }
    return .full;
}

pub fn testOsClose(fd: std.posix.fd_t) void {
    // Zig 0.16 removed std.posix.close; raw-fd close routes through Io.File on
    // the process-global blocking io (paired with `testOsPipe`).
    var file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = false } };
    file.close(globalIo());
}

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    // Zig 0.16 removed std.posix.write; raw-fd writes route through Io.File on
    // the process-global blocking io (`common.globalIo`) — the io-free path for
    // the forked child, outside the daemon's threaded io spine.
    const io = globalIo();
    var file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = false } };
    try file.writeStreamingAll(io, bytes);
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "writeFrame/readFrame round-trip an activity frame then EOF" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);

    try writeFrame(fds[1], .activity, "{\"hello\":1}");
    testOsClose(fds[1]); // EOF after one frame

    const far_deadline = clock.nowMillis() + 5_000;
    const out = try readFrame(std.testing.allocator, fds[0], far_deadline, 1024);
    try std.testing.expect(out == .frame);
    try std.testing.expectEqual(FrameType.activity, out.frame.ftype);
    try std.testing.expectEqualStrings("{\"hello\":1}", out.frame.payload);
    std.testing.allocator.free(out.frame.payload);

    const eof = try readFrame(std.testing.allocator, fds[0], far_deadline, 1024);
    try std.testing.expect(eof == .eof);
}

test "readFrame distinguishes activity from result frames in order" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);
    try writeFrame(fds[1], .activity, "a");
    try writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    testOsClose(fds[1]);

    const dl = clock.nowMillis() + 5_000;
    const f1 = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expectEqual(FrameType.activity, f1.frame.ftype);
    std.testing.allocator.free(f1.frame.payload);
    const f2 = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expectEqual(FrameType.result, f2.frame.ftype);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", f2.frame.payload);
    std.testing.allocator.free(f2.frame.payload);
}

test "writeFrame/readFrame round-trip a memory frame" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);
    try writeFrame(fds[1], .memory, "[{\"key\":\"k\",\"content\":\"c\",\"category\":\"core\"}]");
    testOsClose(fds[1]);

    const dl = clock.nowMillis() + 5_000;
    const out = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expectEqual(FrameType.memory, out.frame.ftype);
    try std.testing.expectEqualStrings("[{\"key\":\"k\",\"content\":\"c\",\"category\":\"core\"}]", out.frame.payload);
    std.testing.allocator.free(out.frame.payload);
}

test "writeFrame/readFrame round-trip the §3 lease + credential frame types" {
    inline for (.{ FrameType.lease, FrameType.credential_request, FrameType.credential_response }) |ft| {
        const fds = try testOsPipe();
        defer testOsClose(fds[0]);
        try writeFrame(fds[1], ft, "{\"integration\":\"github\"}");
        testOsClose(fds[1]);
        const dl = clock.nowMillis() + 5_000;
        const out = try readFrame(std.testing.allocator, fds[0], dl, 1024);
        defer std.testing.allocator.free(out.frame.payload);
        try std.testing.expectEqual(ft, out.frame.ftype);
        try std.testing.expectEqualStrings("{\"integration\":\"github\"}", out.frame.payload);
    }
}

test "readFrame returns timed_out when the deadline is already past" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);
    defer testOsClose(fds[1]);
    // No bytes written; a past deadline must not block.
    const out = try readFrame(std.testing.allocator, fds[0], clock.nowMillis() - 1, 1024);
    try std.testing.expect(out == .timed_out);
}

test "readFrame rejects a frame larger than max_payload" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);
    defer testOsClose(fds[1]);
    try writeFrame(fds[1], .activity, "0123456789");
    const dl = clock.nowMillis() + 5_000;
    try std.testing.expectError(error.FrameTooLarge, readFrame(std.testing.allocator, fds[0], dl, 4));
}

test "UsageSnapshot encode/decode round-trips over a usage frame" {
    const fds = try testOsPipe();
    defer testOsClose(fds[0]);
    const snap = UsageSnapshot{ .input_tokens = 7, .cached_input_tokens = 1, .output_tokens = 3 };
    const payload = snap.encode();
    try writeFrame(fds[1], .usage, &payload);
    testOsClose(fds[1]);

    const dl = clock.nowMillis() + 5_000;
    const out = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    defer std.testing.allocator.free(out.frame.payload);
    try std.testing.expectEqual(FrameType.usage, out.frame.ftype);
    try std.testing.expectEqual(snap, UsageSnapshot.decode(out.frame.payload).?);
}
