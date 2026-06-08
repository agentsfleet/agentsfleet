//! Edge/failure tests for the length-prefixed pipe framing (`pipe_proto`),
//! mirroring the in-file tests' real-pipe harness: a zero-length payload (just
//! the header) must round-trip with type preserved, and an EOF that arrives
//! mid-payload (fewer bytes than the header claimed) must surface as a
//! TruncatedFrame — never a clean EOF or a partial frame. A real pipe pair makes
//! the read path exact with no I/O fakes.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pipe_proto = @import("pipe_proto.zig");
const BYTES_PER_KIB = 1024;

const FrameType = pipe_proto.FrameType;

test "readFrame should round-trip and frame an empty-payload activity message" {
    const fds = try pipe_proto.osPipe();
    defer pipe_proto.osClose(fds[0]);

    try pipe_proto.writeFrame(fds[1], .activity, ""); // header only, zero-length body
    pipe_proto.osClose(fds[1]); // EOF after the one frame

    const dl = clock.nowMillis() + 5_000;
    const out = try pipe_proto.readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expect(out == .frame);
    try std.testing.expectEqual(FrameType.activity, out.frame.ftype);
    try std.testing.expectEqual(@as(usize, 0), out.frame.payload.len);
    std.testing.allocator.free(out.frame.payload);

    // A clean frame boundary follows: the next read is a clean EOF, not a frame.
    const eof = try pipe_proto.readFrame(std.testing.allocator, fds[0], dl, 1024);
    try std.testing.expect(eof == .eof);
}

test "readFrame should return TruncatedFrame when EOF arrives mid-payload" {
    const fds = try pipe_proto.osPipe();
    defer pipe_proto.osClose(fds[0]);

    // Hand-write a header claiming 100 bytes, then only 50 bytes of body, then
    // close: the reader fills the header, then hits EOF partway through the body.
    var header: [5]u8 = undefined;
    header[0] = @intFromEnum(FrameType.activity);
    std.mem.writeInt(u32, header[1..5], 100, .big);
    try writeAll(fds[1], &header);
    try writeAll(fds[1], &([_]u8{'x'} ** 50));
    pipe_proto.osClose(fds[1]); // EOF mid-payload (50 of 100)

    const dl = clock.nowMillis() + 5_000;
    try std.testing.expectError(
        error.TruncatedFrame,
        pipe_proto.readFrame(std.testing.allocator, fds[0], dl, BYTES_PER_KIB),
    );
}

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    // Zig 0.16 removed std.posix.write; raw-fd writes route through Io.File on
    // the process-global blocking io (`common.globalIo`).
    const io = common.globalIo();
    var file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = false } };
    try file.writeStreamingAll(io, bytes);
}
