//! The `fleet:{id}:activity` pub/sub channel name — formatted in one place,
//! parsed in one place.
//!
//! Both SSE handlers meet here: the per-fleet tail formats one name, and the
//! workspace multiplex formats one per readable fleet and parses the name back
//! into a fleet id when a frame arrives (a shared consumer's frames carry the
//! channel they came from, not a fleet id in the payload).
//!
//! Stateless namespace — no type owns a channel; a channel name is a value.

const std = @import("std");

pub const PREFIX = "fleet:";
pub const SUFFIX = ":activity";

/// Scratch size for one channel name: prefix + a UUID + suffix, with room to
/// spare. `format` rejects anything longer rather than truncating into a name
/// that would silently subscribe to the wrong channel.
pub const BUF_LEN: usize = 128;

pub const FormatError = error{ChannelTooLong};

/// Write `fleet:{fleet_id}:activity` into `buf` and return the written slice.
/// The result BORROWS `buf` (a `std.fmt.bufPrint` view) — nothing is owned or
/// allocated, so no free is implied.
pub fn format(buf: []u8, fleet_id: []const u8) FormatError![]const u8 {
    // discipline: ok — returns a borrowed view into `buf` (bufPrint), not owned
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{ PREFIX, fleet_id, SUFFIX }) catch
        error.ChannelTooLong;
}

/// The inverse: recover the fleet id from a channel name. Null when the name
/// is not an activity channel (a frame the caller must drop, never mis-route).
pub fn fleetId(channel_name: []const u8) ?[]const u8 {
    if (channel_name.len <= PREFIX.len + SUFFIX.len) return null;
    if (!std.mem.startsWith(u8, channel_name, PREFIX)) return null;
    if (!std.mem.endsWith(u8, channel_name, SUFFIX)) return null;
    return channel_name[PREFIX.len .. channel_name.len - SUFFIX.len];
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

test "format: builds the activity channel for a fleet" {
    var buf: [BUF_LEN]u8 = undefined;
    try testing.expectEqualStrings(
        "fleet:" ++ FLEET_ID ++ ":activity",
        try format(&buf, FLEET_ID),
    );
}

test "format: refuses a name that would not fit rather than truncating" {
    var buf: [8]u8 = undefined;
    try testing.expectError(error.ChannelTooLong, format(&buf, FLEET_ID));
}

test "fleetId: round-trips a formatted channel" {
    var buf: [BUF_LEN]u8 = undefined;
    const channel = try format(&buf, FLEET_ID);
    try testing.expectEqualStrings(FLEET_ID, fleetId(channel).?);
}

test "fleetId: rejects a name that is not an activity channel" {
    try testing.expect(fleetId("fleet:z1:events") == null);
    try testing.expect(fleetId("other:z1:activity") == null);
    try testing.expect(fleetId("") == null);
    // prefix and suffix present but nothing between them — no fleet id to route to
    try testing.expect(fleetId("fleet::activity") == null);
}
