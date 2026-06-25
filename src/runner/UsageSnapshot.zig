//! UsageSnapshot.zig — cumulative token-usage snapshot riding a `usage` frame.
//!
//! Three little-endian u64s (input, cached-input, output), field names matching
//! the renew/report wire (RULE UFS). Counts are cumulative for the run, never
//! deltas; `fold` keeps the per-field maximum so a regressed or replayed frame can
//! never walk the parent's counters backwards (the server's GREATEST clamp is the
//! second guard). File-as-struct so the wire type owns its own encode/decode/fold
//! + drift guard; `pipe_proto` re-exports it as `pipe_proto.UsageSnapshot`.

const UsageSnapshot = @This();
const std = @import("std");

input_tokens: u64 = 0,
cached_input_tokens: u64 = 0,
output_tokens: u64 = 0,

/// Fixed wire size of an encoded snapshot payload.
pub const WIRE_LEN: usize = 3 * @sizeOf(u64);

comptime {
    // Wire-format drift guard: adding a field changes the struct size and must
    // force the encode/decode pair (and WIRE_LEN) to be revisited.
    std.debug.assert(@sizeOf(UsageSnapshot) == WIRE_LEN);
}

pub fn encode(self: UsageSnapshot) [WIRE_LEN]u8 {
    var buf: [WIRE_LEN]u8 = undefined;
    std.mem.writeInt(u64, buf[0..8], self.input_tokens, .little);
    std.mem.writeInt(u64, buf[8..16], self.cached_input_tokens, .little);
    std.mem.writeInt(u64, buf[16..24], self.output_tokens, .little);
    return buf;
}

/// Null unless the payload is exactly `WIRE_LEN` bytes — a malformed frame is
/// dropped by the caller, which keeps its last-known counters (never invented,
/// never reset).
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

// ── tests ────────────────────────────────────────────────────────────────────

test "encode/decode round-trips a snapshot byte-for-byte" {
    const snap = UsageSnapshot{ .input_tokens = 7, .cached_input_tokens = 1, .output_tokens = 3 };
    const payload = snap.encode();
    try std.testing.expectEqual(@as(usize, WIRE_LEN), payload.len);
    try std.testing.expectEqual(snap, decode(&payload).?);
}

test "decode rejects any payload that is not exactly the wire length" {
    const short = [_]u8{0} ** (WIRE_LEN - 1);
    const long = [_]u8{0} ** (WIRE_LEN + 1);
    try std.testing.expect(decode(&short) == null);
    try std.testing.expect(decode(&long) == null);
    try std.testing.expect(decode(&[_]u8{}) == null);
}

test "fold keeps the per-field maximum so counters never regress" {
    var acc = UsageSnapshot{ .input_tokens = 100, .cached_input_tokens = 5, .output_tokens = 40 };
    // A regressed/replayed frame must not walk any counter backwards.
    acc.fold(.{ .input_tokens = 50, .cached_input_tokens = 0, .output_tokens = 20 });
    try std.testing.expectEqual(UsageSnapshot{ .input_tokens = 100, .cached_input_tokens = 5, .output_tokens = 40 }, acc);
    // A growing frame advances each field to the new maximum.
    acc.fold(.{ .input_tokens = 120, .cached_input_tokens = 9, .output_tokens = 40 });
    try std.testing.expectEqual(UsageSnapshot{ .input_tokens = 120, .cached_input_tokens = 9, .output_tokens = 40 }, acc);
}
