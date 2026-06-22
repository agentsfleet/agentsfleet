//! nfnetlink_rule_test.zig — the golden oracle for the rule builders.
//!
//! Each test reconstructs the exact wire bytes real `nft` sent the kernel from
//! the embedded `--debug=mnl` capture (`tests/fixtures/runner/network/captured/*.mnl.txt`,
//! wired as named @embedFile imports by src/build/fixtures.zig; see that dir's
//! README.md for provenance) and requires our builder's payload —
//! everything after the 16-byte `nlmsghdr` (whose seq/flags legitimately
//! differ) — to match byte-for-byte. No transcription: the fixture file IS the
//! expected value, parsed at test time. A parser regression cannot pass
//! silently — an empty expectation can never equal a non-empty payload.
//!
//! mnl dump shape (one 4-byte wire row per line):
//!   |  0000000376  |  | message length |     <- decimal nlmsghdr rows (skipped)
//!   | 02566 | R--- |  |  type | flags  |     <- message select (NEWRULE = 2566)
//!   | 01 00 00 00  |  |  extra header  |     <- nfgenmsg: collection starts
//!   |00014|--|00001|  |len |flags| type|     <- attr header (N flag = nested)
//!   | 75 7a 5f 65  |  |      data      |     <- payload, padded to 4
//!   ----------------                         <- message end: collection stops

const NFT_MSG_NEWRULE_ROW = "| 02566 |";
const NLMSG_HDRLEN = 16;
const NLA_F_NESTED: u16 = 0x8000;

/// Wire bytes of the first NEWRULE message in `dump`, from `nfgenmsg` on.
/// Caller owns the returned slice.
fn expectedRulePayload(alloc: std.mem.Allocator, dump: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var in_msg = false;
    var collecting = false;
    var lines = std.mem.splitScalar(u8, dump, '\n');
    while (lines.next()) |line| {
        if (!in_msg) {
            in_msg = std.mem.indexOf(u8, line, NFT_MSG_NEWRULE_ROW) != null;
            continue;
        }
        if (!collecting) {
            if (std.mem.indexOf(u8, line, "extra header") != null) {
                collecting = true;
                try appendHexRow(alloc, &out, line);
            }
            continue;
        }
        if (std.mem.startsWith(u8, line, "----")) break; // end of message
        if (std.mem.indexOf(u8, line, "|len |flags| type|") != null) {
            try appendAttrHeader(alloc, &out, line);
        } else if (std.mem.indexOf(u8, line, "|      data      |") != null) {
            try appendHexRow(alloc, &out, line);
        }
    }
    return out.toOwnedSlice(alloc);
}

/// `| 75 7a 5f 65  | ...` — append the row's (up to) four hex bytes.
fn appendHexRow(alloc: std.mem.Allocator, out: *std.ArrayList(u8), line: []const u8) !void {
    const close = std.mem.indexOfPos(u8, line, 1, "|") orelse return error.BadFixture;
    var toks = std.mem.tokenizeScalar(u8, line[1..close], ' ');
    while (toks.next()) |tok| {
        if (tok.len != 2) return error.BadFixture;
        try out.append(alloc, try std.fmt.parseInt(u8, tok, 16));
    }
}

/// `|00014|--|00001| ...` — synthesize the 4-byte nlattr header (len, type;
/// the dump's `N` flag maps to NLA_F_NESTED).
fn appendAttrHeader(alloc: std.mem.Allocator, out: *std.ArrayList(u8), line: []const u8) !void {
    var fields = std.mem.splitScalar(u8, line, '|');
    _ = fields.next(); // leading empty
    const len_s = fields.next() orelse return error.BadFixture;
    const flags_s = fields.next() orelse return error.BadFixture;
    const type_s = fields.next() orelse return error.BadFixture;
    const nla_len = try std.fmt.parseInt(u16, len_s, 10);
    var nla_type = try std.fmt.parseInt(u16, type_s, 10);
    if (std.mem.indexOfScalar(u8, flags_s, 'N') != null) nla_type |= NLA_F_NESTED;
    var hdr: [4]u8 = undefined;
    std.mem.writeInt(u16, hdr[0..2], nla_len, .little);
    std.mem.writeInt(u16, hdr[2..4], nla_type, .little);
    try out.appendSlice(alloc, &hdr);
}

fn expectMatchesOracle(dump: []const u8, built: []const u8) !void {
    const alloc = std.testing.allocator;
    const expected = try expectedRulePayload(alloc, dump);
    defer alloc.free(expected);
    try std.testing.expectEqualSlices(u8, expected, built[NLMSG_HDRLEN..]);
}

// Constants mirror the captured ruleset (capture.sh / Plan.zig worker 0).
const TABLE = "uz_egress";
const FWD = "egress_fwd";
const NAT = "egress_nat";
const IFNAME = "uzveth0";
const SET = "allow0";
const SET_ID: u32 = 1;
const SEQ: u32 = 1; // excluded from comparison (lives in the nlmsghdr)

test "newRuleDnsDrop(udp) byte-matches the nft oracle" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rule.newRuleDnsDrop(&mb, TABLE, FWD, IFNAME, .udp, SEQ);
    try expectMatchesOracle(@embedFile("06_rule_drop_dns_udp.mnl.txt"), try mb.finish());
}

test "newRuleDnsDrop(tcp) byte-matches the nft oracle" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rule.newRuleDnsDrop(&mb, TABLE, FWD, IFNAME, .tcp, SEQ);
    try expectMatchesOracle(@embedFile("07_rule_drop_dns_tcp.mnl.txt"), try mb.finish());
}

test "newRuleAllowSet byte-matches the nft oracle" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rule.newRuleAllowSet(&mb, TABLE, FWD, IFNAME, SET, SET_ID, SEQ);
    try expectMatchesOracle(@embedFile("08_rule_allow_set.mnl.txt"), try mb.finish());
}

test "newRuleCtReturn byte-matches the nft oracle" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rule.newRuleCtReturn(&mb, TABLE, FWD, IFNAME, SEQ);
    try expectMatchesOracle(@embedFile("09_rule_ct_return.mnl.txt"), try mb.finish());
}

test "newRuleMasquerade byte-matches the nft oracle" {
    if (native_endian != .little) return error.SkipZigTest;
    var buf: [512]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rule.newRuleMasquerade(&mb, TABLE, NAT, .{ 10, 69, 0, 0 }, 30, IFNAME, SEQ);
    try expectMatchesOracle(@embedFile("10_rule_masquerade.mnl.txt"), try mb.finish());
}

test "fixture parser fails loudly on a malformed dump" {
    const alloc = std.testing.allocator;
    // A NEWRULE marker with a garbage data row must error, not return empty.
    const bad = "| 02566 | R--- |\n| zz zz zz zz  |\t|  extra header  |\n";
    try std.testing.expectError(error.InvalidCharacter, expectedRulePayload(alloc, bad));
}

const MessageBuilder = @import("MessageBuilder.zig");
const rule = @import("nfnetlink_rule.zig");
const std = @import("std");
const native_endian = @import("builtin").cpu.arch.endian();
