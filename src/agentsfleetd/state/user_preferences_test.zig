//! Unit coverage for the pref-key registry. The registry is the entire
//! server-side validation surface for an otherwise-opaque value, so the
//! parse boundary gets its own tests: an unrecognised key must never resolve,
//! and every tag must round-trip through the wire spelling it is named after.

const std = @import("std");
const prefs = @import("user_preferences.zig");

test "every registry key round-trips through its wire spelling" {
    for (std.meta.tags(prefs.PrefKey)) |key| {
        const wire = key.wire();
        const parsed = prefs.PrefKey.fromWire(wire) orelse {
            std.debug.print("key {s} did not parse back\n", .{wire});
            return error.TestUnexpectedResult;
        };
        try std.testing.expectEqual(key, parsed);
    }
}

test "the three onboarding keys are the registry" {
    try std.testing.expectEqual(@as(usize, 3), std.meta.tags(prefs.PrefKey).len);
    try std.testing.expectEqual(
        prefs.PrefKey.getting_started_dismissed,
        prefs.PrefKey.fromWire("getting_started_dismissed").?,
    );
    try std.testing.expectEqual(
        prefs.PrefKey.getting_started_collapsed,
        prefs.PrefKey.fromWire("getting_started_collapsed").?,
    );
    try std.testing.expectEqual(
        prefs.PrefKey.getting_started_cli_ticked,
        prefs.PrefKey.fromWire("getting_started_cli_ticked").?,
    );
}

test "a key outside the registry does not resolve" {
    try std.testing.expect(prefs.PrefKey.fromWire("bogus") == null);
    try std.testing.expect(prefs.PrefKey.fromWire("") == null);
    // Prototype-shaped and near-miss spellings must miss too — the allowlist is
    // the only thing standing between a client and an unbounded key space.
    try std.testing.expect(prefs.PrefKey.fromWire("constructor") == null);
    try std.testing.expect(prefs.PrefKey.fromWire("getting_started_dismiss") == null);
    try std.testing.expect(prefs.PrefKey.fromWire("GETTING_STARTED_DISMISSED") == null);
}

test "the value cap is the documented 1 KiB" {
    // The cap is published in the OpenAPI description and in the UZ-PREFS-002
    // hint, so a silent change here would make both of those lie.
    // pin test: literal is the contract
    try std.testing.expectEqual(@as(usize, 1024), prefs.MAX_PREF_VALUE_BYTES);
}
