//! MessageBuilder.zig — a netlink message serializer (pure, fixed-buffer).
//!
//! Accumulates one netlink message into a caller-provided buffer: the 16-byte
//! `nlmsghdr`, an optional family header, and 4-byte-aligned TLV attributes
//! (`nlattr`), with nesting for nftables expression trees. `finish` patches
//! `nlmsg_len` and returns the framed slice. No syscalls — the `rtnetlink` /
//! `nfnetlink` builders write through this and `Socket` sends the bytes (Linux).
//! An `std.io`-style accumulating serializer; golden-byte tested on every target.
//!
//! Netlink framing is native-endian (the kernel reads headers in host order);
//! attribute *payloads* carry their own endianness — rtnetlink mostly native,
//! nftables mostly big-endian — so payloads arrive here pre-encoded by the
//! caller and this builder only frames them.

const MessageBuilder = @This();

buf: []u8,
len: usize = 0,
/// Set when a put would exceed `buf`; `finish` then returns `BufferTooSmall`
/// rather than corrupting memory in a release build (no capacity `assert`).
overflow: bool = false,

pub const Error = error{BufferTooSmall};

// `nlmsghdr` flags + attribute markers the rt/nf builders pass in.
pub const NLM_F_REQUEST: u16 = 0x01;
pub const NLM_F_ACK: u16 = 0x04;
pub const NLM_F_EXCL: u16 = 0x200;
pub const NLM_F_CREATE: u16 = 0x400;
pub const NLA_F_NESTED: u16 = 0x8000;

const NLMSG_HDRLEN: usize = 16;
const NLA_HDRLEN: usize = 4;

pub fn init(buf: []u8) MessageBuilder {
    return .{ .buf = buf };
}

/// Begin a message: write the `nlmsghdr` (length patched by `finish`, pid 0).
pub fn start(self: *MessageBuilder, msg_type: u16, flags: u16, seq: u32) void {
    self.len = 0;
    self.overflow = false;
    self.putU32(0); // nlmsg_len placeholder
    self.putU16(msg_type);
    self.putU16(flags);
    self.putU32(seq);
    self.putU32(0); // nlmsg_pid — 0 addresses the kernel
}

/// Append a family header struct (caller-encoded, e.g. `ifinfomsg`), 4-aligned.
pub fn familyHeader(self: *MessageBuilder, h: []const u8) void {
    self.putBytes(h);
    self.pad4();
}

/// One TLV attribute: `nlattr{len,type}` + payload, padded to 4 bytes.
pub fn attr(self: *MessageBuilder, attr_type: u16, payload: []const u8) void {
    self.putU16(@intCast(NLA_HDRLEN + payload.len));
    self.putU16(attr_type);
    self.putBytes(payload);
    self.pad4();
}

/// A native-endian u32 attribute (the common rtnetlink shape).
pub fn attrU32(self: *MessageBuilder, attr_type: u16, value: u32) void {
    var b: [4]u8 = undefined;
    std.mem.writeInt(u32, &b, value, native_endian);
    self.attr(attr_type, &b);
}

/// A NUL-terminated string attribute (interface / table / chain names).
pub fn attrStr(self: *MessageBuilder, attr_type: u16, s: []const u8) void {
    self.putU16(@intCast(NLA_HDRLEN + s.len + 1));
    self.putU16(attr_type);
    self.putBytes(s);
    self.putU8(0);
    self.pad4();
}

/// Open a nested attribute (nftables expressions); returns a mark for `nestEnd`.
pub fn nestBegin(self: *MessageBuilder, attr_type: u16) usize {
    const mark = self.len;
    self.putU16(0); // nla_len placeholder
    self.putU16(attr_type | NLA_F_NESTED);
    return mark;
}

/// Close the nested attribute opened at `mark`, patching its length in place.
pub fn nestEnd(self: *MessageBuilder, mark: usize) void {
    if (self.overflow or mark + 2 > self.buf.len) return;
    std.mem.writeInt(u16, self.buf[mark..][0..2], @intCast(self.len - mark), native_endian);
}

/// Patch `nlmsg_len` and return the framed message, or error on overflow.
pub fn finish(self: *MessageBuilder) Error![]const u8 {
    if (self.overflow) return error.BufferTooSmall;
    std.mem.writeInt(u32, self.buf[0..4], @intCast(self.len), native_endian);
    return self.buf[0..self.len];
}

// ── internal putters — bounds-checked; set `overflow` instead of asserting ──

fn putU8(self: *MessageBuilder, v: u8) void {
    if (!self.ensure(1)) return;
    self.buf[self.len] = v;
    self.len += 1;
}

fn putU16(self: *MessageBuilder, v: u16) void {
    if (!self.ensure(2)) return;
    std.mem.writeInt(u16, self.buf[self.len..][0..2], v, native_endian);
    self.len += 2;
}

fn putU32(self: *MessageBuilder, v: u32) void {
    if (!self.ensure(4)) return;
    std.mem.writeInt(u32, self.buf[self.len..][0..4], v, native_endian);
    self.len += 4;
}

fn putBytes(self: *MessageBuilder, b: []const u8) void {
    if (!self.ensure(b.len)) return;
    @memcpy(self.buf[self.len..][0..b.len], b);
    self.len += b.len;
}

fn pad4(self: *MessageBuilder) void {
    const pad = (4 - (self.len % 4)) % 4;
    var i: usize = 0;
    while (i < pad) : (i += 1) self.putU8(0);
}

fn ensure(self: *MessageBuilder, n: usize) bool {
    if (self.len + n > self.buf.len) {
        self.overflow = true;
        return false;
    }
    return true;
}

// ── Tests (golden bytes; little-endian targets — all of ours) ───────────────

test "header-only message frames nlmsghdr with patched length" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [64]u8 = undefined;
    var mb = init(&buf);
    mb.start(0x0010, NLM_F_REQUEST, 5);
    const msg = try mb.finish();
    try std.testing.expectEqualSlices(u8, &[_]u8{
        0x10, 0, 0, 0, // nlmsg_len = 16
        0x10, 0, // nlmsg_type = 0x0010
        0x01, 0, // nlmsg_flags = REQUEST
        0x05, 0, 0, 0, // nlmsg_seq = 5
        0x00, 0, 0, 0, // nlmsg_pid = 0
    }, msg);
}

test "u32 attribute frames nlattr header + little-endian payload" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [64]u8 = undefined;
    var mb = init(&buf);
    mb.start(0, 0, 0);
    mb.attrU32(0x0003, 0xAABBCCDD);
    const msg = try mb.finish();
    try std.testing.expectEqualSlices(u8, &[_]u8{
        0x18, 0, 0, 0, // nlmsg_len = 24
        0x00, 0, 0x00, 0, 0x00, 0, 0, 0, 0x00, 0, 0, 0, // type/flags/seq/pid
        0x08, 0, // nla_len = 8
        0x03, 0, // nla_type = 3
        0xDD, 0xCC, 0xBB, 0xAA, // u32 0xAABBCCDD little-endian
    }, msg);
}

test "string attribute is NUL-terminated and 4-byte padded" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [64]u8 = undefined;
    var mb = init(&buf);
    mb.start(0, 0, 0);
    mb.attrStr(0x0001, "ab"); // 4 hdr + 2 + 1 NUL = 7, padded to 8
    const msg = try mb.finish();
    const attr_bytes = msg[NLMSG_HDRLEN..];
    try std.testing.expectEqualSlices(u8, &[_]u8{
        0x07, 0, // nla_len = 7 (header + "ab\0", pre-padding)
        0x01, 0, // nla_type = 1
        'a', 'b', 0x00, // value + NUL
        0x00, // pad to 8
    }, attr_bytes);
    try std.testing.expectEqual(@as(usize, NLMSG_HDRLEN + 8), msg.len);
}

test "nested attribute length is patched to cover its children" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = init(&buf);
    mb.start(0, 0, 0);
    const mark = mb.nestBegin(0x0002);
    mb.attrU32(0x0001, 0); // 8 bytes inside the nest
    mb.nestEnd(mark);
    const msg = try mb.finish();
    // nested nla_len = 4 (its own header) + 8 (child) = 12
    const nested_len = std.mem.readInt(u16, msg[NLMSG_HDRLEN..][0..2], .little);
    try std.testing.expectEqual(@as(u16, 12), nested_len);
    const nested_type = std.mem.readInt(u16, msg[NLMSG_HDRLEN + 2 ..][0..2], .little);
    try std.testing.expectEqual(@as(u16, 0x0002 | NLA_F_NESTED), nested_type);
}

test "overflow on a too-small buffer fails closed at finish" {
    var buf: [8]u8 = undefined; // smaller than the 16-byte header
    var mb = init(&buf);
    mb.start(0, 0, 0);
    try std.testing.expectError(error.BufferTooSmall, mb.finish());
}

test "every message is 4-byte aligned in total length" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = init(&buf);
    mb.start(0, 0, 0);
    mb.attrStr(1, "eth0"); // odd-ish content to force padding paths
    mb.attrU32(2, 42);
    const msg = try mb.finish();
    try std.testing.expectEqual(@as(usize, 0), msg.len % 4);
}

const std = @import("std");
const native_endian = @import("builtin").cpu.arch.endian();
