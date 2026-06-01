//! pipe_proto.zig — typed length-prefixed framing over the lease pipe.
//!
//! The child multiplexes two message classes onto its stdout: zero-or-more
//! `activity` frames streamed during execution (live-tail progress), then
//! exactly one terminal `result` frame. The parent reads frames in order,
//! forwards each `activity` frame to the control plane, and parses the `result`
//! frame as the `ExecutionResult`. stdout is the one fd that crosses the bwrap
//! boundary cleanly, so the progress channel rides it rather than a fragile
//! extra descriptor.
//!
//! Frame = [1 byte type][4 byte big-endian length][payload]. The payload is
//! opaque to this module (the writer serializes, the reader hands bytes back);
//! framing owns only the envelope. Reads are bounded by the lease wall-clock
//! deadline so a stuck child cannot block the parent past `lease_expires_at`.

const std = @import("std");

/// Header byte selecting the message class. Values are ASCII so a stray frame is
/// legible in a hexdump; the enum is the single source (RULE UFS).
pub const FrameType = enum(u8) {
    activity = 'A',
    result = 'R',
};

const HEADER_LEN = 1 + 4; // type byte + u32 big-endian length

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

    const ftype = std.meta.intToEnum(FrameType, header[0]) catch return error.UnknownFrameType;
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
    const remaining = deadline_ms - std.time.milliTimestamp();
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
        const remaining = deadline_ms - std.time.milliTimestamp();
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

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var off: usize = 0;
    while (off < bytes.len) off += try std.posix.write(fd, bytes[off..]);
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "writeFrame/readFrame round-trip an activity frame then EOF" {
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);

    try writeFrame(fds[1], .activity, "{\"hello\":1}");
    std.posix.close(fds[1]); // EOF after one frame

    const far_deadline = std.time.milliTimestamp() + 5_000;
    const out = try readFrame(std.testing.allocator, fds[0], far_deadline, 1024);
    try std.testing.expect(out == .frame);
    try std.testing.expectEqual(FrameType.activity, out.frame.ftype);
    try std.testing.expectEqualStrings("{\"hello\":1}", out.frame.payload);
    std.testing.allocator.free(out.frame.payload);

    const eof = try readFrame(std.testing.allocator, fds[0], far_deadline, 1024);
    try std.testing.expect(eof == .eof);
}

test "readFrame distinguishes activity from result frames in order" {
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    try writeFrame(fds[1], .activity, "a");
    try writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    std.posix.close(fds[1]);

    const dl = std.time.milliTimestamp() + 5_000;
    const f1 = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expectEqual(FrameType.activity, f1.frame.ftype);
    std.testing.allocator.free(f1.frame.payload);
    const f2 = try readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expectEqual(FrameType.result, f2.frame.ftype);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", f2.frame.payload);
    std.testing.allocator.free(f2.frame.payload);
}

test "readFrame returns timed_out when the deadline is already past" {
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    defer std.posix.close(fds[1]);
    // No bytes written; a past deadline must not block.
    const out = try readFrame(std.testing.allocator, fds[0], std.time.milliTimestamp() - 1, 1024);
    try std.testing.expect(out == .timed_out);
}

test "readFrame rejects a frame larger than max_payload" {
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    defer std.posix.close(fds[1]);
    try writeFrame(fds[1], .activity, "0123456789");
    const dl = std.time.milliTimestamp() + 5_000;
    try std.testing.expectError(error.FrameTooLarge, readFrame(std.testing.allocator, fds[0], dl, 4));
}
