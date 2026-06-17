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
const common = @import("common");
const clock = common.clock;
const globalIo = common.globalIo;

/// Header byte selecting the message class. Values are ASCII so a stray frame is
/// legible in a hexdump; the enum is the single source (RULE UFS).
pub const FrameType = enum(u8) {
    activity = 'A',
    result = 'R',
    memory = 'M',
    usage = 'U',
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

/// Cumulative token-usage snapshot riding a `usage` frame: three little-endian
/// u64s (input, cached-input, output), field names matching the renew/report
/// wire (RULE UFS). Counts are cumulative for the run, never deltas; `fold`
/// keeps the per-field maximum so a regressed or replayed frame can never walk
/// the parent's counters backwards (the server's GREATEST clamp is the second
/// guard).
pub const UsageSnapshot = struct {
    input_tokens: u64 = 0,
    cached_input_tokens: u64 = 0,
    output_tokens: u64 = 0,

    /// Fixed wire size of an encoded snapshot payload.
    pub const WIRE_LEN: usize = 3 * @sizeOf(u64);

    comptime {
        // Wire-format drift guard: adding a field changes the struct size and
        // must force the encode/decode pair (and WIRE_LEN) to be revisited.
        std.debug.assert(@sizeOf(UsageSnapshot) == WIRE_LEN);
    }

    pub fn encode(self: UsageSnapshot) [WIRE_LEN]u8 {
        var buf: [WIRE_LEN]u8 = undefined;
        std.mem.writeInt(u64, buf[0..8], self.input_tokens, .little);
        std.mem.writeInt(u64, buf[8..16], self.cached_input_tokens, .little);
        std.mem.writeInt(u64, buf[16..24], self.output_tokens, .little);
        return buf;
    }

    /// Null unless the payload is exactly `WIRE_LEN` bytes — a malformed frame
    /// is dropped by the caller, which keeps its last-known counters (never
    /// invented, never reset).
    pub fn decode(payload: []const u8) ?UsageSnapshot {
        if (payload.len != WIRE_LEN) return null;
        return .{
            .input_tokens = std.mem.readInt(u64, payload[0..8], .little),
            .cached_input_tokens = std.mem.readInt(u64, payload[8..16], .little),
            .output_tokens = std.mem.readInt(u64, payload[16..24], .little),
        };
    }

    /// Per-field maximum fold — cumulative counters only ever grow.
    pub fn fold(self: *UsageSnapshot, other: UsageSnapshot) void {
        self.input_tokens = @max(self.input_tokens, other.input_tokens);
        self.cached_input_tokens = @max(self.cached_input_tokens, other.cached_input_tokens);
        self.output_tokens = @max(self.output_tokens, other.output_tokens);
    }
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

test "UsageSnapshot.decode rejects any payload that is not exactly the wire length" {
    const short = [_]u8{0} ** (UsageSnapshot.WIRE_LEN - 1);
    const long = [_]u8{0} ** (UsageSnapshot.WIRE_LEN + 1);
    try std.testing.expect(UsageSnapshot.decode(&short) == null);
    try std.testing.expect(UsageSnapshot.decode(&long) == null);
    try std.testing.expect(UsageSnapshot.decode(&.{}) == null);
}

test "UsageSnapshot.fold keeps the per-field maximum so counters never regress" {
    var acc = UsageSnapshot{};
    acc.fold(.{ .input_tokens = 100, .cached_input_tokens = 0, .output_tokens = 40 });
    try std.testing.expectEqual(@as(u64, 100), acc.input_tokens);
    // A regressed (restarted-child) snapshot must not walk anything backwards.
    acc.fold(.{ .input_tokens = 50, .cached_input_tokens = 2, .output_tokens = 20 });
    try std.testing.expectEqual(@as(u64, 100), acc.input_tokens);
    try std.testing.expectEqual(@as(u64, 2), acc.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 40), acc.output_tokens);
}
