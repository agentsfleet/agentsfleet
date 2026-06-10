//! nfnetlink.zig — nftables (`NETLINK_NETFILTER`) structural builders.
//!
//! A stateless function namespace over `MessageBuilder`: the transaction frame
//! (`batchBegin`/`batchEnd`) and the structural objects — table, chain (with its
//! hook + base policy), set, and set elements. The rule + expression encoding
//! (`iifname @set accept`, default drop, masquerade) lives in its own file
//! (`nfnetlink_rule.zig`) because the expression VM is intricate and the two
//! together would exceed the file-length limit.
//!
//! nftables attribute integers are BIG-ENDIAN (network order) — unlike
//! rtnetlink's native-endian — so `attrU32Be` is the integer path here.
//! `nfgenmsg.res_id` is big-endian too. Pure; `Socket` sends, the integration
//! lane proves kernel acceptance.

const MessageBuilder = @import("MessageBuilder.zig");
const std = @import("std");

const NFNL_SUBSYS_NFTABLES: u16 = 10;
/// Compose a full nlmsg type from an NFT_MSG_* id (shared with the rule builder).
pub fn nftType(msg: u16) u16 {
    return (NFNL_SUBSYS_NFTABLES << 8) | msg;
}

// NFT_MSG_* (linux/netfilter/nf_tables.h).
const NFT_MSG_NEWTABLE: u16 = 0;
const NFT_MSG_NEWCHAIN: u16 = 3;
const NFT_MSG_NEWSET: u16 = 9;
const NFT_MSG_NEWSETELEM: u16 = 12;
// Batch envelope (NFNL_SUBSYS_NONE).
const NFNL_MSG_BATCH_BEGIN: u16 = 16;
const NFNL_MSG_BATCH_END: u16 = 17;

// Families + verdicts the caller passes to `newChain` (linux/netfilter.h).
pub const NFPROTO_UNSPEC: u8 = 0;
pub const NFPROTO_INET: u8 = 1;
pub const NF_DROP: u32 = 0;
pub const NF_ACCEPT: u32 = 1;
pub const NF_INET_FORWARD: u32 = 2;
pub const NF_INET_POST_ROUTING: u32 = 4;

// NFTA_* attribute ids.
const NFTA_TABLE_NAME: u16 = 1;
const NFTA_CHAIN_TABLE: u16 = 1;
const NFTA_CHAIN_NAME: u16 = 3;
const NFTA_CHAIN_HOOK: u16 = 4;
const NFTA_CHAIN_POLICY: u16 = 5;
const NFTA_CHAIN_TYPE: u16 = 7;
const NFTA_HOOK_HOOKNUM: u16 = 1;
const NFTA_HOOK_PRIORITY: u16 = 2;
const NFTA_SET_TABLE: u16 = 1;
const NFTA_SET_NAME: u16 = 2;
const NFTA_SET_FLAGS: u16 = 3;
const NFTA_SET_KEY_TYPE: u16 = 4;
const NFTA_SET_KEY_LEN: u16 = 5;
const NFTA_SET_ID: u16 = 10;
const NFTA_SET_ELEM_LIST_TABLE: u16 = 1;
const NFTA_SET_ELEM_LIST_SET: u16 = 2;
const NFTA_SET_ELEM_LIST_ELEMENTS: u16 = 3;
const NFTA_LIST_ELEM: u16 = 1;
const NFTA_SET_ELEM_KEY: u16 = 1;
const NFTA_DATA_VALUE: u16 = 1;

const IPV4_KEY_LEN: u32 = 4;
// nft datatype id for ipv4_addr keys (TYPE_IPADDR) — mandatory in NEWSET; the
// oracle capture (fixtures/captured/04_set_allow0.mnl.txt) proved its absence
// was a real-nft divergence (Codex's NFTA_SET_KEY_TYPE catch).
const TYPE_IPADDR: u32 = 7;
const REQ_CREATE = MessageBuilder.NLM_F_REQUEST | MessageBuilder.NLM_F_CREATE;

/// `struct nfgenmsg` (4 bytes): family, version (NFNETLINK_V0), res_id (big-endian).
/// Shared with the rule builder (`nfnetlink_rule.zig`).
pub fn nfgenmsg(family: u8, res_id: u16) [4]u8 {
    return .{ family, 0, @intCast(res_id >> 8), @intCast(res_id & 0xff) };
}

/// A big-endian u32 attribute — the nftables integer path. Shared with the
/// rule builder (`nfnetlink_rule.zig`).
pub fn attrU32Be(mb: *MessageBuilder, attr_type: u16, value: u32) void {
    var b: [4]u8 = undefined;
    std.mem.writeInt(u32, &b, value, .big);
    mb.attr(attr_type, &b);
}

/// Open a transaction. nftables object changes must sit between this and
/// `batchEnd`, all on consecutive sequence numbers.
pub fn batchBegin(mb: *MessageBuilder, seq: u32) void {
    mb.start(NFNL_MSG_BATCH_BEGIN, MessageBuilder.NLM_F_REQUEST, seq);
    const g = nfgenmsg(NFPROTO_UNSPEC, NFNL_SUBSYS_NFTABLES);
    mb.familyHeader(&g);
}

/// Commit the transaction opened by `batchBegin`.
pub fn batchEnd(mb: *MessageBuilder, seq: u32) void {
    mb.start(NFNL_MSG_BATCH_END, MessageBuilder.NLM_F_REQUEST, seq);
    const g = nfgenmsg(NFPROTO_UNSPEC, NFNL_SUBSYS_NFTABLES);
    mb.familyHeader(&g);
}

/// Create an `inet` table `name`.
pub fn newTable(mb: *MessageBuilder, name: []const u8, seq: u32) void {
    mb.start(nftType(NFT_MSG_NEWTABLE), REQ_CREATE, seq);
    const g = nfgenmsg(NFPROTO_INET, 0);
    mb.familyHeader(&g);
    mb.attrStr(NFTA_TABLE_NAME, name);
}

/// Create a base `filter` chain on `hooknum`/`priority` with base `policy`.
pub fn newChain(
    mb: *MessageBuilder,
    table: []const u8,
    name: []const u8,
    hooknum: u32,
    priority: i32,
    policy: u32,
    seq: u32,
) void {
    mb.start(nftType(NFT_MSG_NEWCHAIN), REQ_CREATE, seq);
    const g = nfgenmsg(NFPROTO_INET, 0);
    mb.familyHeader(&g);
    mb.attrStr(NFTA_CHAIN_TABLE, table);
    mb.attrStr(NFTA_CHAIN_NAME, name);
    const hook = mb.nestBegin(NFTA_CHAIN_HOOK);
    attrU32Be(mb, NFTA_HOOK_HOOKNUM, hooknum);
    attrU32Be(mb, NFTA_HOOK_PRIORITY, @bitCast(priority));
    mb.nestEnd(hook);
    mb.attrStr(NFTA_CHAIN_TYPE, "filter");
    attrU32Be(mb, NFTA_CHAIN_POLICY, policy);
}

/// Create a named IPv4-key set (`set_id` ties later elements to this set within
/// the same transaction).
pub fn newSet(mb: *MessageBuilder, table: []const u8, name: []const u8, set_id: u32, seq: u32) void {
    mb.start(nftType(NFT_MSG_NEWSET), REQ_CREATE, seq);
    const g = nfgenmsg(NFPROTO_INET, 0);
    mb.familyHeader(&g);
    mb.attrStr(NFTA_SET_TABLE, table);
    mb.attrStr(NFTA_SET_NAME, name);
    attrU32Be(mb, NFTA_SET_FLAGS, 0);
    attrU32Be(mb, NFTA_SET_KEY_TYPE, TYPE_IPADDR);
    attrU32Be(mb, NFTA_SET_KEY_LEN, IPV4_KEY_LEN);
    attrU32Be(mb, NFTA_SET_ID, set_id);
}

/// Add IPv4 `addrs` as elements of set `name` (the allowlist).
pub fn addSetElems(mb: *MessageBuilder, table: []const u8, name: []const u8, addrs: []const [4]u8, seq: u32) void {
    mb.start(nftType(NFT_MSG_NEWSETELEM), REQ_CREATE, seq);
    const g = nfgenmsg(NFPROTO_INET, 0);
    mb.familyHeader(&g);
    mb.attrStr(NFTA_SET_ELEM_LIST_TABLE, table);
    mb.attrStr(NFTA_SET_ELEM_LIST_SET, name);
    const elements = mb.nestBegin(NFTA_SET_ELEM_LIST_ELEMENTS);
    for (addrs) |a| {
        const elem = mb.nestBegin(NFTA_LIST_ELEM);
        const key = mb.nestBegin(NFTA_SET_ELEM_KEY);
        mb.attr(NFTA_DATA_VALUE, &a);
        mb.nestEnd(key);
        mb.nestEnd(elem);
    }
    mb.nestEnd(elements);
}

// ── Tests (golden bytes; little-endian targets) ─────────────────────────────

test "newTable frames the nftables subsystem type + inet family + name" {
    if (@import("builtin").cpu.arch.endian() != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newTable(&mb, "uz_egress", 1);
    const msg = try mb.finish();
    try std.testing.expectEqual(@as(u16, 0x0A00), std.mem.readInt(u16, msg[4..6], .little)); // (10<<8)|0
    try std.testing.expectEqual(NFPROTO_INET, msg[16]); // nfgenmsg.family
    try std.testing.expect(std.mem.indexOf(u8, msg, "uz_egress") != null);
}

test "newSet encodes an ipv4_addr key type and key_len of 4" {
    if (@import("builtin").cpu.arch.endian() != .little) return error.SkipZigTest;
    var buf: [128]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newSet(&mb, "uz_egress", "allow0", 1, 1);
    const msg = try mb.finish();
    // attr hdr {len 8, type KEY_TYPE} + TYPE_IPADDR=7 big-endian — the full
    // 8-byte TLV, byte-matching the oracle (04_set_allow0.mnl.txt).
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 8, 0, 4, 0, 0, 0, 0, 7 }) != null);
    // KEY_LEN=4 big-endian appears as its full TLV right after.
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 8, 0, 5, 0, 0, 0, 0, 4 }) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "allow0") != null);
}

test "addSetElems carries each IPv4 address as an element key" {
    if (@import("builtin").cpu.arch.endian() != .little) return error.SkipZigTest;
    var buf: [256]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    const addrs = [_][4]u8{ .{ 1, 2, 3, 4 }, .{ 10, 69, 0, 1 } };
    addSetElems(&mb, "uz_egress", "allow0", &addrs, 1);
    const msg = try mb.finish();
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 1, 2, 3, 4 }) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 10, 69, 0, 1 }) != null);
}

test "newChain carries a big-endian forward hooknum and drop policy" {
    if (@import("builtin").cpu.arch.endian() != .little) return error.SkipZigTest;
    var buf: [256]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newChain(&mb, "uz_egress", "egress_fwd", NF_INET_FORWARD, 0, NF_DROP, 1);
    const msg = try mb.finish();
    try std.testing.expectEqual(@as(u16, 0x0A03), std.mem.readInt(u16, msg[4..6], .little)); // (10<<8)|3
    // hooknum=2 big-endian = 00 00 00 02 present (inside NFTA_CHAIN_HOOK).
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 0, 0, 0, 2 }) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "filter") != null);
}
