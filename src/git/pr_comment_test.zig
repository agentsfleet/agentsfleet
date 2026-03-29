//! Unit tests for PR comment helpers (M16_001 §3.4).

const std = @import("std");
const pr = @import("pr.zig");

// ---------------------------------------------------------------------------
// T1 — Happy path
// ---------------------------------------------------------------------------

test "extractPrNumber returns number from valid PR URL" {
    const result = pr.extractPrNumber("https://github.com/usezombie/zombie/pull/42");
    try std.testing.expect(result != null);
    try std.testing.expectEqualStrings("42", result.?);
}

test "extractPrNumber handles large PR numbers" {
    const result = pr.extractPrNumber("https://github.com/org/repo/pull/99999");
    try std.testing.expect(result != null);
    try std.testing.expectEqualStrings("99999", result.?);
}

// ---------------------------------------------------------------------------
// T2 — Edge cases
// ---------------------------------------------------------------------------

test "extractPrNumber returns null for non-PR URL" {
    try std.testing.expect(pr.extractPrNumber("https://github.com/org/repo") == null);
}

test "extractPrNumber returns null for empty string" {
    try std.testing.expect(pr.extractPrNumber("") == null);
}

test "extractPrNumber returns null for PR URL with non-numeric suffix" {
    try std.testing.expect(pr.extractPrNumber("https://github.com/org/repo/pull/abc") == null);
}

test "extractPrNumber returns null for trailing slash after number" {
    // "/pull/42/" has a trailing slash — the '/' is not a digit
    try std.testing.expect(pr.extractPrNumber("https://github.com/org/repo/pull/42/") == null);
}

test "extractPrNumber handles single digit PR" {
    const result = pr.extractPrNumber("https://github.com/org/repo/pull/1");
    try std.testing.expect(result != null);
    try std.testing.expectEqualStrings("1", result.?);
}

// ---------------------------------------------------------------------------
// T3 — Negative paths
// ---------------------------------------------------------------------------

test "extractPrNumber returns null when pull marker is missing" {
    try std.testing.expect(pr.extractPrNumber("https://github.com/org/repo/issues/42") == null);
}

test "extractPrNumber returns null for empty number after pull/" {
    try std.testing.expect(pr.extractPrNumber("https://github.com/org/repo/pull/") == null);
}
