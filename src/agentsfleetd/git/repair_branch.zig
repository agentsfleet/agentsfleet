//! Compact repair-branch identity.
//!
//! A repair branch carries the approved repository-write gate, encoded from
//! its 16 raw Universally Unique Identifier version 7 (UUIDv7) bytes as
//! unpadded URL-safe Base64. The branch never embeds
//! Fleet or event identifiers; the daemon resolves those through the gate row.

const std = @import("std");
const id_format = @import("../types/id_format.zig");

pub const PREFIX = "agentsfleet-repair/";
pub const REFERENCE_LEN: usize = 22;
pub const BRANCH_LEN: usize = PREFIX.len + REFERENCE_LEN;
const base64 = std.base64.url_safe_no_pad;

pub const Error = error{InvalidRepairBranch};
const TEST_GATE_ID = "0197a4ba-8d3a-7f13-8abc-123456789abc";

/// Build the complete daemon-authored repair branch from a canonical UUIDv7.
pub fn fromGateId(gate_id: []const u8) Error![BRANCH_LEN]u8 {
    const raw = id_format.uuidV7ToBytes(gate_id) orelse return error.InvalidRepairBranch;
    var out: [BRANCH_LEN]u8 = undefined;
    @memcpy(out[0..PREFIX.len], PREFIX);
    _ = base64.Encoder.encode(out[PREFIX.len..], &raw);
    return out;
}

/// Recover the canonical gate UUIDv7 from one exact repair branch.
pub fn gateId(branch: []const u8) Error![id_format.UUID_TEXT_LEN]u8 {
    if (branch.len != BRANCH_LEN or !std.mem.startsWith(u8, branch, PREFIX)) {
        return error.InvalidRepairBranch;
    }
    const reference = branch[PREFIX.len..];
    var raw: [id_format.UUID_BYTE_LEN]u8 = undefined;
    base64.Decoder.decode(&raw, reference) catch return error.InvalidRepairBranch;
    const gate_id = id_format.uuidV7FromBytes(raw) orelse return error.InvalidRepairBranch;

    var canonical_reference: [REFERENCE_LEN]u8 = undefined;
    _ = base64.Encoder.encode(&canonical_reference, &raw);
    if (!std.mem.eql(u8, reference, &canonical_reference)) return error.InvalidRepairBranch;
    return gate_id;
}

test "test_repair_branch_uses_compact_gate_reference" {
    const branch = try fromGateId(TEST_GATE_ID);
    try std.testing.expectEqual(@as(usize, BRANCH_LEN), branch.len);
    try std.testing.expectEqual(@as(usize, REFERENCE_LEN), branch[PREFIX.len..].len);
    try std.testing.expect(std.mem.indexOfScalar(u8, &branch, '=') == null);
    try std.testing.expect(std.mem.indexOfScalar(u8, &branch, '+') == null);
    const decoded = try gateId(&branch);
    try std.testing.expectEqualStrings(TEST_GATE_ID, &decoded);
}

test "repair branch rejects aliases and non-UUIDv7 references" {
    const branch = try fromGateId(TEST_GATE_ID);
    try std.testing.expectError(error.InvalidRepairBranch, gateId(branch[0 .. branch.len - 1]));

    var padded: [BRANCH_LEN + 1]u8 = undefined;
    @memcpy(padded[0..BRANCH_LEN], &branch);
    padded[BRANCH_LEN] = '=';
    try std.testing.expectError(error.InvalidRepairBranch, gateId(&padded));

    var noncanonical = branch;
    const last = BRANCH_LEN - 1;
    noncanonical[last] = switch (noncanonical[last]) {
        'A' => 'B',
        'Q' => 'R',
        'g' => 'h',
        'w' => 'x',
        else => unreachable,
    };
    try std.testing.expectError(error.InvalidRepairBranch, gateId(&noncanonical));

    try std.testing.expectError(
        error.InvalidRepairBranch,
        fromGateId("0197a4ba-8d3a-6f13-8abc-123456789abc"),
    );
    try std.testing.expectError(
        error.InvalidRepairBranch,
        fromGateId("0197A4BA-8D3A-7F13-8ABC-123456789ABC"),
    );
    try std.testing.expectError(
        error.InvalidRepairBranch,
        fromGateId("0197a4ba-8d3a-7f13-7abc-123456789abc"),
    );
}
