//! Socket.zig — the AF_NETLINK kernel touch (the ONLY Linux-gated file in
//! `network/`; everything else is pure and Mac-testable).
//!
//! Owns one netlink datagram socket. `roundTrip` sends the caller's serialized
//! frame(s) and requires a positive kernel ack per command: NLM_F_ACK is
//! patched into each command header HERE — wire-delivery semantics are this
//! file's concern; the pure builders stay byte-faithful to the nft oracle.
//! A negative `NLMSG_ERROR` is the kernel refusing a command (`KernelRefused`)
//! and the caller fails the lease closed. Every recv is bounded by a socket
//! timeout: a silent kernel becomes `RecvFailed`, never a hung worker.

const Socket = @This();

fd: i32,

/// Netlink protocol numbers (linux/netlink.h).
pub const Protocol = enum(u32) { route = 0, netfilter = 12 };

pub const Error = error{
    UnsupportedPlatform,
    OpenFailed,
    SendFailed,
    RecvFailed,
    KernelRefused,
    MalformedFrame,
};

const RECV_TIMEOUT_S: isize = 5;
const ACK_BUF_LEN = 4096;
const NLMSG_HDRLEN = 16;
const NLMSG_ERROR_TYPE: u16 = 2;
// NFNL batch envelope messages — the kernel never acks these two.
const NFNL_MSG_BATCH_BEGIN: u16 = 16;
const NFNL_MSG_BATCH_END: u16 = 17;

/// Open a netlink socket for `proto`. The runner needs CAP_NET_ADMIN for the
/// messages it sends; opening succeeds without it, the kernel refuses later.
pub fn open(proto: Protocol) Error!Socket {
    if (builtin.os.tag != .linux) return error.UnsupportedPlatform;
    const linux = std.os.linux;
    const rc = linux.socket(linux.AF.NETLINK, linux.SOCK.RAW | linux.SOCK.CLOEXEC, @intFromEnum(proto));
    if (linux.errno(rc) != .SUCCESS) return error.OpenFailed;
    const fd: i32 = @intCast(rc);
    // Bound every recv (fail closed, never hang the lease worker).
    const tv = linux.timeval{ .sec = RECV_TIMEOUT_S, .usec = 0 };
    _ = linux.setsockopt(fd, linux.SOL.SOCKET, linux.SO.RCVTIMEO, @ptrCast(&tv), @sizeOf(linux.timeval));
    return .{ .fd = fd };
}

/// Idempotent close.
pub fn close(self: *Socket) void {
    if (builtin.os.tag == .linux and self.fd >= 0) _ = std.os.linux.close(self.fd);
    self.fd = -1;
}

/// Send the back-to-back nlmsg `frames` and consume one kernel ack per
/// command message. Mutates `frames` (the ACK-flag patch).
pub fn roundTrip(self: Socket, frames: []u8) Error!void {
    if (builtin.os.tag != .linux) return error.UnsupportedPlatform;
    const linux = std.os.linux;
    const expected = try markAcksExpected(frames);

    var sa = linux.sockaddr.nl{ .pid = 0, .groups = 0 }; // pid 0 = the kernel
    const sent = linux.sendto(self.fd, frames.ptr, frames.len, 0, @ptrCast(&sa), @sizeOf(linux.sockaddr.nl));
    if (linux.errno(sent) != .SUCCESS) return error.SendFailed;

    var acked: u32 = 0;
    var rbuf: [ACK_BUF_LEN]u8 = undefined;
    while (acked < expected) {
        const rc = linux.recvfrom(self.fd, &rbuf, rbuf.len, 0, null, null);
        if (linux.errno(rc) != .SUCCESS) return error.RecvFailed; // EAGAIN = timeout stop path
        acked += try countAcks(rbuf[0..rc]);
    }
}

/// Patch NLM_F_ACK into every command header in `frames` (batch envelopes
/// excluded — the kernel does not ack them) and return how many acks to
/// expect. Pure; the cross-platform half of `roundTrip`.
fn markAcksExpected(frames: []u8) Error!u32 {
    var expected: u32 = 0;
    var off: usize = 0;
    while (off + NLMSG_HDRLEN <= frames.len) {
        const mlen = std.mem.readInt(u32, frames[off..][0..4], native_endian);
        if (mlen < NLMSG_HDRLEN or off + mlen > frames.len) return error.MalformedFrame;
        const mtype = std.mem.readInt(u16, frames[off + 4 ..][0..2], native_endian);
        if (mtype != NFNL_MSG_BATCH_BEGIN and mtype != NFNL_MSG_BATCH_END) {
            const flags = std.mem.readInt(u16, frames[off + 6 ..][0..2], native_endian);
            std.mem.writeInt(u16, frames[off + 6 ..][0..2], flags | MessageBuilder.NLM_F_ACK, native_endian);
            expected += 1;
        }
        off += std.mem.alignForward(usize, mlen, 4);
    }
    if (off != frames.len) return error.MalformedFrame;
    return expected;
}

/// Count NLMSG_ERROR acks in one recv'd datagram; a negative error number is
/// the kernel refusing a command. Pure.
fn countAcks(buf: []const u8) Error!u32 {
    var acks: u32 = 0;
    var off: usize = 0;
    while (off + NLMSG_HDRLEN <= buf.len) {
        const mlen = std.mem.readInt(u32, buf[off..][0..4], native_endian);
        if (mlen < NLMSG_HDRLEN or off + mlen > buf.len) return error.MalformedFrame;
        const mtype = std.mem.readInt(u16, buf[off + 4 ..][0..2], native_endian);
        if (mtype == NLMSG_ERROR_TYPE) {
            if (mlen < NLMSG_HDRLEN + 4) return error.MalformedFrame;
            const err_no = std.mem.readInt(i32, buf[off + NLMSG_HDRLEN ..][0..4], native_endian);
            if (err_no != 0) {
                log.err("kernel_refused", .{ .error_code = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED, .errno = err_no });
                return error.KernelRefused;
            }
            acks += 1;
        }
        off += std.mem.alignForward(usize, mlen, 4);
    }
    return acks;
}

// ── Tests (the pure frame-walk halves; `open` itself is the integration lane) ─

test "open is fail-closed off Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectError(error.UnsupportedPlatform, open(.route));
}

test "markAcksExpected patches ACK into commands and skips batch envelopes" {
    var buf: [256]u8 = undefined;
    var frames: [256]u8 = undefined;
    var flen: usize = 0;
    // batch begin + one table command + batch end, packed back-to-back.
    var mb = MessageBuilder.init(&buf);
    nfnetlink.batchBegin(&mb, 0);
    flen += copyMsg(frames[flen..], try mb.finish());
    nfnetlink.newTable(&mb, "uz_egress", 1);
    flen += copyMsg(frames[flen..], try mb.finish());
    nfnetlink.batchEnd(&mb, 2);
    flen += copyMsg(frames[flen..], try mb.finish());

    const expected = try markAcksExpected(frames[0..flen]);
    try std.testing.expectEqual(@as(u32, 1), expected);
    // The command (second message) now carries NLM_F_ACK; the envelopes don't.
    const first_flags = std.mem.readInt(u16, frames[6..8], native_endian);
    try std.testing.expectEqual(@as(u16, 0), first_flags & MessageBuilder.NLM_F_ACK);
}

test "markAcksExpected rejects a truncated frame" {
    var frames = [_]u8{ 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 };
    try std.testing.expectError(error.MalformedFrame, markAcksExpected(&frames));
}

test "countAcks counts zero-errno acks and refuses on a kernel error" {
    // Hand-frame two NLMSG_ERROR messages: errno 0 (ack), then errno -1.
    var buf: [40]u8 = undefined;
    for ([_]i32{ 0, -1 }, 0..) |err_no, i| {
        const off = i * 20;
        std.mem.writeInt(u32, buf[off..][0..4], 20, native_endian);
        std.mem.writeInt(u16, buf[off + 4 ..][0..2], NLMSG_ERROR_TYPE, native_endian);
        std.mem.writeInt(u16, buf[off + 6 ..][0..2], 0, native_endian);
        std.mem.writeInt(u32, buf[off + 8 ..][0..4], 1, native_endian);
        std.mem.writeInt(u32, buf[off + 12 ..][0..4], 0, native_endian);
        std.mem.writeInt(i32, buf[off + 16 ..][0..4], err_no, native_endian);
    }
    try std.testing.expectEqual(@as(u32, 1), countAcks(buf[0..20]));
    try std.testing.expectError(error.KernelRefused, countAcks(buf[0..40]));
}

fn copyMsg(dst: []u8, msg: []const u8) usize {
    @memcpy(dst[0..msg.len], msg);
    return msg.len;
}

const MessageBuilder = @import("MessageBuilder.zig");
const nfnetlink = @import("nfnetlink.zig");
const std = @import("std");
const builtin = @import("builtin");
const native_endian = builtin.cpu.arch.endian();
const log = @import("log").scoped(.egress_socket);
const client_errors = @import("../engine/client_errors.zig");
