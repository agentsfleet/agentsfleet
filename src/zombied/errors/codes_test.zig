// Tests for hint and credential error codes (migrated from codes.zig for M16_001).

const std = @import("std");
const ec = @import("error_registry.zig");

test "hint returns actionable text for known startup codes" {
    try std.testing.expect(ec.hint(ec.ERR_STARTUP_REDIS_CONNECT).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_INTERNAL_DB_UNAVAILABLE).len > 0);
}

test "hint returns UNKNOWN hint for unregistered codes" {
    // audit-error-codes: intentional-fake
    const h = ec.hint("UZ-NONEXISTENT-999");
    try std.testing.expectEqualStrings(ec.UNKNOWN.hint, h);
}
