//! nfnetlink_rule.zig — nftables rule + expression-VM encoding.
//!
//! The danger-zone half of the nftables builders (`nfnetlink.zig` holds the
//! structural objects): the four per-lease rules — DNS-tunnel drop, allowlist
//! accept, conntrack return, masquerade — each serialized as the expression
//! program real `nft` emits. Byte-validated against the captured oracle in
//! `tests/fixtures/runner/network/captured/*.mnl.txt` (see `nfnetlink_rule_test.zig`); structural
//! invariants are tested in-file. Pure; `Socket` sends.
//!
//! Encoding notes pinned by the oracle: expression u32 attributes are
//! BIG-ENDIAN; cmp operands are natural-width raw bytes (1-byte nfproto /
//! l4proto, 2-byte be16 port, 4-byte network-order address, 16-byte padded
//! ifname) — except conntrack-state words, which are host-endian register
//! values; attribute order inside each expression follows nft, not ascending
//! attribute id (`lookup` writes SREG before SET).

const MessageBuilder = @import("MessageBuilder.zig");
const nfnetlink = @import("nfnetlink.zig");
const std = @import("std");
const native_endian = @import("builtin").cpu.arch.endian();

const NFT_MSG_NEWRULE: u16 = 6;
const NFTA_RULE_TABLE: u16 = 1;
const NFTA_RULE_CHAIN: u16 = 2;
const NFTA_RULE_EXPRESSIONS: u16 = 4;
const NFTA_LIST_ELEM: u16 = 1;
const NFTA_EXPR_NAME: u16 = 1;
const NFTA_EXPR_DATA: u16 = 2;

const NFTA_META_DREG: u16 = 1;
const NFTA_META_KEY: u16 = 2;
const NFT_META_IIFNAME: u32 = 6;
const NFT_META_OIFNAME: u32 = 7;
const NFT_META_NFPROTO: u32 = 15;
const NFT_META_L4PROTO: u32 = 16;

const NFTA_CMP_SREG: u16 = 1;
const NFTA_CMP_OP: u16 = 2;
const NFTA_CMP_DATA: u16 = 3;
const NFT_CMP_EQ: u32 = 0;
const NFT_CMP_NEQ: u32 = 1;

const NFTA_PAYLOAD_DREG: u16 = 1;
const NFTA_PAYLOAD_BASE: u16 = 2;
const NFTA_PAYLOAD_OFFSET: u16 = 3;
const NFTA_PAYLOAD_LEN: u16 = 4;
const NFT_PAYLOAD_NETWORK_HEADER: u32 = 1;
const NFT_PAYLOAD_TRANSPORT_HEADER: u32 = 2;

const NFTA_LOOKUP_SET: u16 = 1;
const NFTA_LOOKUP_SREG: u16 = 2;
const NFTA_LOOKUP_SET_ID: u16 = 4;

const NFTA_IMMEDIATE_DREG: u16 = 1;
const NFTA_IMMEDIATE_DATA: u16 = 2;
const NFTA_DATA_VALUE: u16 = 1;
const NFTA_DATA_VERDICT: u16 = 2;
const NFTA_VERDICT_CODE: u16 = 1;

const NFTA_BITWISE_SREG: u16 = 1;
const NFTA_BITWISE_DREG: u16 = 2;
const NFTA_BITWISE_LEN: u16 = 3;
const NFTA_BITWISE_MASK: u16 = 4;
const NFTA_BITWISE_XOR: u16 = 5;

const NFTA_CT_DREG: u16 = 1;
const NFTA_CT_KEY: u16 = 2;
const NFT_CT_STATE: u32 = 0;
const CT_STATE_ESTABLISHED: u32 = 2;
const CT_STATE_RELATED: u32 = 4;

const NFT_REG_VERDICT: u32 = 0;
const NFT_REG_1: u32 = 1;

// Kernel cmp operand widths/offsets (linux/in.h, ip header layout).
const IFNAME_CMP_LEN = 16;
const NFPROTO_IPV4: u8 = 2;
const IP_OFFSET_SADDR: u32 = 12;
const IP_OFFSET_DADDR: u32 = 16;
const ADDR_LEN: u32 = 4;
const TRANSPORT_OFFSET_DPORT: u32 = 2;
const PORT_LEN: u32 = 2;
const DNS_PORT: u16 = 53;

const REQ_ADD = MessageBuilder.NLM_F_REQUEST | MessageBuilder.NLM_F_CREATE |
    MessageBuilder.NLM_F_APPEND;

/// IANA protocol numbers for the two DNS transports we drop.
pub const L4Proto = enum(u8) { tcp = 6, udp = 17 };

/// `iifname <if> {udp|tcp} dport 53 drop` — the DNS-tunnel closure. Sits
/// BEFORE the allowlist accept so even an allowlisted IP is no resolver.
pub fn newRuleDnsDrop(
    mb: *MessageBuilder,
    table: []const u8,
    chain: []const u8,
    iifname: []const u8,
    proto: L4Proto,
    seq: u32,
) void {
    const exprs = ruleBegin(mb, table, chain, seq);
    exprMetaLoad(mb, NFT_META_IIFNAME);
    exprCmpIfname(mb, NFT_CMP_EQ, iifname);
    exprMetaLoad(mb, NFT_META_L4PROTO);
    exprCmp(mb, NFT_CMP_EQ, &[_]u8{@intFromEnum(proto)});
    exprPayload(mb, NFT_PAYLOAD_TRANSPORT_HEADER, TRANSPORT_OFFSET_DPORT, PORT_LEN);
    var port: [2]u8 = undefined;
    std.mem.writeInt(u16, &port, DNS_PORT, .big);
    exprCmp(mb, NFT_CMP_EQ, &port);
    exprVerdict(mb, nfnetlink.NF_DROP);
    mb.nestEnd(exprs);
}

/// `iifname <if> ip daddr @<set> accept` — the allowlist itself.
pub fn newRuleAllowSet(
    mb: *MessageBuilder,
    table: []const u8,
    chain: []const u8,
    iifname: []const u8,
    set_name: []const u8,
    set_id: u32,
    seq: u32,
) void {
    const exprs = ruleBegin(mb, table, chain, seq);
    exprMetaLoad(mb, NFT_META_IIFNAME);
    exprCmpIfname(mb, NFT_CMP_EQ, iifname);
    exprMetaLoad(mb, NFT_META_NFPROTO);
    exprCmp(mb, NFT_CMP_EQ, &[_]u8{NFPROTO_IPV4});
    exprPayload(mb, NFT_PAYLOAD_NETWORK_HEADER, IP_OFFSET_DADDR, ADDR_LEN);
    exprLookup(mb, set_name, set_id);
    exprVerdict(mb, nfnetlink.NF_ACCEPT);
    mb.nestEnd(exprs);
}

/// `oifname <if> ct state established,related accept` — return traffic
/// toward the child survives the chain's drop policy.
pub fn newRuleCtReturn(
    mb: *MessageBuilder,
    table: []const u8,
    chain: []const u8,
    oifname: []const u8,
    seq: u32,
) void {
    const exprs = ruleBegin(mb, table, chain, seq);
    exprMetaLoad(mb, NFT_META_OIFNAME);
    exprCmpIfname(mb, NFT_CMP_EQ, oifname);
    exprCtStateLoad(mb);
    var mask: [4]u8 = undefined; // ct state is a host-endian register word
    std.mem.writeInt(u32, &mask, CT_STATE_ESTABLISHED | CT_STATE_RELATED, native_endian);
    exprBitwiseMask4(mb, mask);
    exprCmp(mb, NFT_CMP_NEQ, &[_]u8{ 0, 0, 0, 0 });
    exprVerdict(mb, nfnetlink.NF_ACCEPT);
    mb.nestEnd(exprs);
}

/// `ip saddr <subnet>/<prefix> oifname != <if> masquerade` — the /30 source
/// NATs out the uplink, never back down its own veth.
pub fn newRuleMasquerade(
    mb: *MessageBuilder,
    table: []const u8,
    chain: []const u8,
    subnet: [4]u8,
    prefix_len: u8,
    not_oifname: []const u8,
    seq: u32,
) void {
    const exprs = ruleBegin(mb, table, chain, seq);
    exprMetaLoad(mb, NFT_META_NFPROTO);
    exprCmp(mb, NFT_CMP_EQ, &[_]u8{NFPROTO_IPV4});
    exprPayload(mb, NFT_PAYLOAD_NETWORK_HEADER, IP_OFFSET_SADDR, ADDR_LEN);
    exprBitwiseMask4(mb, prefixMask(prefix_len));
    exprCmp(mb, NFT_CMP_EQ, &subnet);
    exprMetaLoad(mb, NFT_META_OIFNAME);
    exprCmpIfname(mb, NFT_CMP_NEQ, not_oifname);
    exprMasq(mb);
    mb.nestEnd(exprs);
}

// ── expression emitters (all load/compare via register 1, like nft) ─────────

fn ruleBegin(mb: *MessageBuilder, table: []const u8, chain: []const u8, seq: u32) usize {
    mb.start(nfnetlink.nftType(NFT_MSG_NEWRULE), REQ_ADD, seq);
    const g = nfnetlink.nfgenmsg(nfnetlink.NFPROTO_INET, 0);
    mb.familyHeader(&g);
    mb.attrStr(NFTA_RULE_TABLE, table);
    mb.attrStr(NFTA_RULE_CHAIN, chain);
    return mb.nestBegin(NFTA_RULE_EXPRESSIONS);
}

const Expr = struct { elem: usize, data: usize };

fn exprBegin(mb: *MessageBuilder, name: []const u8) Expr {
    const elem = mb.nestBegin(NFTA_LIST_ELEM);
    mb.attrStr(NFTA_EXPR_NAME, name);
    return .{ .elem = elem, .data = mb.nestBegin(NFTA_EXPR_DATA) };
}

fn exprEnd(mb: *MessageBuilder, e: Expr) void {
    mb.nestEnd(e.data);
    mb.nestEnd(e.elem);
}

fn exprMetaLoad(mb: *MessageBuilder, key: u32) void {
    const e = exprBegin(mb, "meta");
    nfnetlink.attrU32Be(mb, NFTA_META_KEY, key); // KEY precedes DREG (oracle)
    nfnetlink.attrU32Be(mb, NFTA_META_DREG, NFT_REG_1);
    exprEnd(mb, e);
}

fn exprCmp(mb: *MessageBuilder, op: u32, data: []const u8) void {
    const e = exprBegin(mb, "cmp");
    nfnetlink.attrU32Be(mb, NFTA_CMP_SREG, NFT_REG_1);
    nfnetlink.attrU32Be(mb, NFTA_CMP_OP, op);
    const d = mb.nestBegin(NFTA_CMP_DATA);
    mb.attr(NFTA_DATA_VALUE, data);
    mb.nestEnd(d);
    exprEnd(mb, e);
}

fn exprCmpIfname(mb: *MessageBuilder, op: u32, ifname: []const u8) void {
    var buf = [_]u8{0} ** IFNAME_CMP_LEN;
    const n = @min(ifname.len, IFNAME_CMP_LEN - 1);
    @memcpy(buf[0..n], ifname[0..n]);
    exprCmp(mb, op, &buf);
}

fn exprPayload(mb: *MessageBuilder, base: u32, offset: u32, len: u32) void {
    const e = exprBegin(mb, "payload");
    nfnetlink.attrU32Be(mb, NFTA_PAYLOAD_DREG, NFT_REG_1);
    nfnetlink.attrU32Be(mb, NFTA_PAYLOAD_BASE, base);
    nfnetlink.attrU32Be(mb, NFTA_PAYLOAD_OFFSET, offset);
    nfnetlink.attrU32Be(mb, NFTA_PAYLOAD_LEN, len);
    exprEnd(mb, e);
}

fn exprLookup(mb: *MessageBuilder, set_name: []const u8, set_id: u32) void {
    const e = exprBegin(mb, "lookup");
    nfnetlink.attrU32Be(mb, NFTA_LOOKUP_SREG, NFT_REG_1); // SREG precedes SET
    mb.attrStr(NFTA_LOOKUP_SET, set_name);
    nfnetlink.attrU32Be(mb, NFTA_LOOKUP_SET_ID, set_id);
    exprEnd(mb, e);
}

fn exprVerdict(mb: *MessageBuilder, verdict: u32) void {
    const e = exprBegin(mb, "immediate");
    nfnetlink.attrU32Be(mb, NFTA_IMMEDIATE_DREG, NFT_REG_VERDICT);
    const d = mb.nestBegin(NFTA_IMMEDIATE_DATA);
    const v = mb.nestBegin(NFTA_DATA_VERDICT);
    nfnetlink.attrU32Be(mb, NFTA_VERDICT_CODE, verdict);
    mb.nestEnd(v);
    mb.nestEnd(d);
    exprEnd(mb, e);
}

fn exprBitwiseMask4(mb: *MessageBuilder, mask: [4]u8) void {
    const e = exprBegin(mb, "bitwise");
    nfnetlink.attrU32Be(mb, NFTA_BITWISE_SREG, NFT_REG_1);
    nfnetlink.attrU32Be(mb, NFTA_BITWISE_DREG, NFT_REG_1);
    nfnetlink.attrU32Be(mb, NFTA_BITWISE_LEN, ADDR_LEN);
    const m = mb.nestBegin(NFTA_BITWISE_MASK);
    mb.attr(NFTA_DATA_VALUE, &mask);
    mb.nestEnd(m);
    const x = mb.nestBegin(NFTA_BITWISE_XOR);
    mb.attr(NFTA_DATA_VALUE, &[_]u8{ 0, 0, 0, 0 });
    mb.nestEnd(x);
    exprEnd(mb, e);
}

fn exprCtStateLoad(mb: *MessageBuilder) void {
    const e = exprBegin(mb, "ct");
    nfnetlink.attrU32Be(mb, NFTA_CT_KEY, NFT_CT_STATE); // KEY precedes DREG
    nfnetlink.attrU32Be(mb, NFTA_CT_DREG, NFT_REG_1);
    exprEnd(mb, e);
}

fn exprMasq(mb: *MessageBuilder) void {
    const elem = mb.nestBegin(NFTA_LIST_ELEM);
    mb.attrStr(NFTA_EXPR_NAME, "masq");
    const data = mb.nestBegin(NFTA_EXPR_DATA); // present-but-empty (oracle)
    mb.nestEnd(data);
    mb.nestEnd(elem);
}

/// Network-order netmask for `prefix_len` leading one-bits (`/30` → ff ff ff fc).
fn prefixMask(prefix_len: u8) [4]u8 {
    const m: u32 = if (prefix_len == 0) 0 else ~@as(u32, 0) << @intCast(32 - @min(prefix_len, 32));
    var b: [4]u8 = undefined;
    std.mem.writeInt(u32, &b, m, .big);
    return b;
}

// ── Tests — structural invariants; the full byte oracle lives in
// nfnetlink_rule_test.zig against tests/fixtures/runner/network/captured/*.mnl.txt ──

test "dns drop rule sequences meta/cmp/meta/cmp/payload/cmp/immediate" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newRuleDnsDrop(&mb, "uz_egress", "egress_fwd", "uzveth0", .udp, 1);
    const msg = try mb.finish();
    var at: usize = 0;
    for ([_][]const u8{ "meta", "cmp", "meta", "cmp", "payload", "cmp", "immediate" }) |name| {
        at = (std.mem.indexOfPos(u8, msg, at, name) orelse return error.TestExpectedEqual) + name.len;
    }
    // be16 port 53 operand present; drop verdict (code 0) carried by immediate.
    try std.testing.expect(std.mem.indexOf(u8, msg, &[_]u8{ 0x00, 0x35 }) != null);
}

test "allow rule looks up the set and accepts" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    newRuleAllowSet(&mb, "uz_egress", "egress_fwd", "uzveth0", "allow0", 1, 1);
    const msg = try mb.finish();
    try std.testing.expect(std.mem.indexOf(u8, msg, "lookup") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "allow0") != null);
    // ifname operand is NUL-padded to exactly 16 bytes.
    const ifname_padded = "uzveth0" ++ [_]u8{0} ** 9;
    try std.testing.expect(std.mem.indexOf(u8, msg, ifname_padded) != null);
}

test "prefixMask renders /30 and the degenerate bounds" {
    try std.testing.expectEqual([4]u8{ 0xff, 0xff, 0xff, 0xfc }, prefixMask(30));
    try std.testing.expectEqual([4]u8{ 0, 0, 0, 0 }, prefixMask(0));
    try std.testing.expectEqual([4]u8{ 0xff, 0xff, 0xff, 0xff }, prefixMask(32));
}
