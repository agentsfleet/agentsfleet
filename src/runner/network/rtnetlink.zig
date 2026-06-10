//! rtnetlink.zig — rtnetlink (`NETLINK_ROUTE`) message builders.
//!
//! A stateless function namespace (the `std.mem` shape — not a `@This()` type):
//! each `fn` frames one rtnetlink request through `MessageBuilder`. Pure; the
//! Linux-only `Socket` sends the bytes and reads the ACK. Covers the veth
//! lifecycle `EgressScope` needs: create a veth pair, move the peer into the
//! child's netns by pid, assign an address, add the default route, bring a link
//! up. Golden-byte tested; the integration lane proves the kernel accepts them.

const MessageBuilder = @import("MessageBuilder.zig");
const std = @import("std");
const native_endian = @import("builtin").cpu.arch.endian();

// Message types (linux/rtnetlink.h).
pub const RTM_NEWLINK: u16 = 16;
pub const RTM_DELLINK: u16 = 17;
pub const RTM_NEWADDR: u16 = 20;
pub const RTM_NEWROUTE: u16 = 24;

const AF_UNSPEC: u8 = 0;
const AF_INET: u8 = 2;

// IFLA_* link attributes (linux/if_link.h).
const IFLA_IFNAME: u16 = 3;
const IFLA_LINKINFO: u16 = 18;
const IFLA_NET_NS_PID: u16 = 19;
const IFLA_INFO_KIND: u16 = 1;
const IFLA_INFO_DATA: u16 = 2;
const VETH_INFO_PEER: u16 = 1;
const IFF_UP: u32 = 1;

// IFA_* address attributes (linux/if_addr.h).
const IFA_ADDRESS: u16 = 1;
const IFA_LOCAL: u16 = 2;

// rtmsg / RTA_* route fields (linux/rtnetlink.h).
const RTA_OIF: u16 = 4;
const RTA_GATEWAY: u16 = 5;
const RT_TABLE_MAIN: u8 = 254;
const RTPROT_BOOT: u8 = 3;
const RT_SCOPE_UNIVERSE: u8 = 0;
const RTN_UNICAST: u8 = 1;

// REQUEST|ACK for a plain set; +CREATE|EXCL for a create-new.
const REQ_ACK = MessageBuilder.NLM_F_REQUEST | MessageBuilder.NLM_F_ACK;
const REQ_NEW = REQ_ACK | MessageBuilder.NLM_F_CREATE | MessageBuilder.NLM_F_EXCL;

/// `struct ifinfomsg` (16 bytes): family, pad, type, index, flags, change.
fn ifinfomsg(family: u8, index: i32, flags: u32, change: u32) [16]u8 {
    var h: [16]u8 = @splat(0);
    h[0] = family;
    std.mem.writeInt(i32, h[4..8], index, native_endian);
    std.mem.writeInt(u32, h[8..12], flags, native_endian);
    std.mem.writeInt(u32, h[12..16], change, native_endian);
    return h;
}

/// `struct ifaddrmsg` (8 bytes): family, prefixlen, flags, scope, index.
fn ifaddrmsg(family: u8, prefixlen: u8, scope: u8, index: u32) [8]u8 {
    var h: [8]u8 = @splat(0);
    h[0] = family;
    h[1] = prefixlen;
    h[3] = scope;
    std.mem.writeInt(u32, h[4..8], index, native_endian);
    return h;
}

/// `struct rtmsg` (12 bytes): family, dst_len, src_len, tos, table, protocol,
/// scope, type, flags.
fn rtmsg(family: u8, table: u8, protocol: u8, scope: u8, rtype: u8) [12]u8 {
    var h: [12]u8 = @splat(0);
    h[0] = family;
    h[4] = table;
    h[5] = protocol;
    h[6] = scope;
    h[7] = rtype;
    return h;
}

/// Create a veth pair `host_ifname <-> peer_ifname` (the nested
/// LINKINFO→INFO_DATA→VETH_INFO_PEER form a single RTM_NEWLINK carries).
pub fn newVethPair(mb: *MessageBuilder, host_ifname: []const u8, peer_ifname: []const u8, seq: u32) void {
    mb.start(RTM_NEWLINK, REQ_NEW, seq);
    const base = ifinfomsg(AF_UNSPEC, 0, 0, 0);
    mb.familyHeader(&base);
    mb.attrStr(IFLA_IFNAME, host_ifname);

    const link_info = mb.nestBegin(IFLA_LINKINFO);
    mb.attrStr(IFLA_INFO_KIND, "veth");
    const info_data = mb.nestBegin(IFLA_INFO_DATA);
    const peer = mb.nestBegin(VETH_INFO_PEER);
    const peer_base = ifinfomsg(AF_UNSPEC, 0, 0, 0);
    mb.familyHeader(&peer_base);
    mb.attrStr(IFLA_IFNAME, peer_ifname);
    mb.nestEnd(peer);
    mb.nestEnd(info_data);
    mb.nestEnd(link_info);
}

/// Move link `index` into the network namespace of process `pid`.
pub fn moveLinkToNetns(mb: *MessageBuilder, index: i32, pid: u32, seq: u32) void {
    mb.start(RTM_NEWLINK, REQ_ACK, seq);
    const base = ifinfomsg(AF_UNSPEC, index, 0, 0);
    mb.familyHeader(&base);
    mb.attrU32(IFLA_NET_NS_PID, pid);
}

/// Bring link `index` administratively up.
pub fn setLinkUp(mb: *MessageBuilder, index: i32, seq: u32) void {
    mb.start(RTM_NEWLINK, REQ_ACK, seq);
    const base = ifinfomsg(AF_UNSPEC, index, IFF_UP, IFF_UP);
    mb.familyHeader(&base);
}

/// Assign IPv4 `addr/prefixlen` to link `index`.
pub fn newAddr(mb: *MessageBuilder, index: u32, addr: [4]u8, prefixlen: u8, seq: u32) void {
    mb.start(RTM_NEWADDR, REQ_NEW, seq);
    const base = ifaddrmsg(AF_INET, prefixlen, RT_SCOPE_UNIVERSE, index);
    mb.familyHeader(&base);
    mb.attr(IFA_LOCAL, &addr);
    mb.attr(IFA_ADDRESS, &addr);
}

/// Delete link `ifname` (either veth end tears down the pair; teardown path).
pub fn delLink(mb: *MessageBuilder, ifname: []const u8, seq: u32) void {
    mb.start(RTM_DELLINK, REQ_ACK, seq);
    const base = ifinfomsg(AF_UNSPEC, 0, 0, 0);
    mb.familyHeader(&base);
    mb.attrStr(IFLA_IFNAME, ifname);
}

/// Add the default route via IPv4 gateway `gw` out interface `oif`.
pub fn newDefaultRoute(mb: *MessageBuilder, oif: u32, gw: [4]u8, seq: u32) void {
    mb.start(RTM_NEWROUTE, REQ_NEW, seq);
    const base = rtmsg(AF_INET, RT_TABLE_MAIN, RTPROT_BOOT, RT_SCOPE_UNIVERSE, RTN_UNICAST);
    mb.familyHeader(&base);
    mb.attr(RTA_GATEWAY, &gw);
    mb.attrU32(RTA_OIF, oif);
}

// ── Tests (golden bytes; little-endian targets) ─────────────────────────────

test "moveLinkToNetns frames RTM_NEWLINK + ifinfomsg(index) + NET_NS_PID" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    moveLinkToNetns(&mb, 7, 4242, 1);
    const msg = try mb.finish();

    const msg_type = std.mem.readInt(u16, msg[4..6], .little);
    try std.testing.expectEqual(RTM_NEWLINK, msg_type);
    // ifinfomsg.index sits at offset 16+4 = 20.
    try std.testing.expectEqual(@as(i32, 7), std.mem.readInt(i32, msg[20..24], .little));
    // The IFLA_NET_NS_PID attr value is the last 4 bytes.
    try std.testing.expectEqual(@as(u32, 4242), std.mem.readInt(u32, msg[msg.len - 4 ..][0..4], .little));
}

test "newAddr frames RTM_NEWADDR + ifaddrmsg + IFA_LOCAL/IFA_ADDRESS" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newAddr(&mb, 7, .{ 10, 69, 0, 1 }, 30, 1);
    const msg = try mb.finish();

    try std.testing.expectEqual(RTM_NEWADDR, std.mem.readInt(u16, msg[4..6], .little));
    // ifaddrmsg: family=AF_INET, prefixlen=30 at offsets 16, 17.
    try std.testing.expectEqual(@as(u8, AF_INET), msg[16]);
    try std.testing.expectEqual(@as(u8, 30), msg[17]);
    // The address bytes appear in both IFA_LOCAL and IFA_ADDRESS payloads.
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 10, 69, 0, 1 }) != null);
}

test "newVethPair is RTM_NEWLINK and carries the veth kind + both names" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [256]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newVethPair(&mb, "uzveth0", "uzveth0p", 1);
    const msg = try mb.finish();

    try std.testing.expectEqual(RTM_NEWLINK, std.mem.readInt(u16, msg[4..6], .little));
    try std.testing.expect((std.mem.readInt(u16, msg[6..8], .little) & MessageBuilder.NLM_F_CREATE) != 0);
    try std.testing.expect(std.mem.indexOf(u8, msg, "veth") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "uzveth0") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "uzveth0p") != null);
}

test "delLink frames RTM_DELLINK with the interface name" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [64]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    delLink(&mb, "uzveth0", 1);
    const msg = try mb.finish();
    try std.testing.expectEqual(RTM_DELLINK, std.mem.readInt(u16, msg[4..6], .little));
    try std.testing.expect(std.mem.indexOf(u8, msg, "uzveth0") != null);
}

test "setLinkUp sets IFF_UP in both flags and change" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [64]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    setLinkUp(&mb, 7, 1);
    const msg = try mb.finish();
    // ifinfomsg.flags at offset 16+8=24, change at 16+12=28.
    try std.testing.expectEqual(IFF_UP, std.mem.readInt(u32, msg[24..28], .little));
    try std.testing.expectEqual(IFF_UP, std.mem.readInt(u32, msg[28..32], .little));
}
