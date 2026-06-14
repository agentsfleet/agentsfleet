// Tests for hint and credential error codes (migrated from codes.zig).

const std = @import("std");
const ec = @import("error_registry.zig");

test "hint returns actionable text for known startup codes" {
    try std.testing.expect(ec.hint(ec.ERR_STARTUP_REDIS_CONNECT).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_INTERNAL_DB_UNAVAILABLE).len > 0);
}

test "no registry entry references a retired worker datastore env var" {
    // UZ-STARTUP-004's hint dropped its REDIS_URL_WORKER mention when the worker
    // datastore substrate was retired. No registry entry may name the dead *_WORKER
    // env vars — an operator would otherwise be sent chasing a var that no longer
    // exists. Covers both the control-plane and execute-path entry tables.
    const retired = [_][]const u8{ "DATABASE_URL_WORKER", "REDIS_URL_WORKER" };
    for (ec.REGISTRY) |entry| {
        for (retired) |needle| {
            try std.testing.expect(std.mem.indexOf(u8, entry.hint, needle) == null);
            try std.testing.expect(std.mem.indexOf(u8, entry.title, needle) == null);
        }
    }
}

test "hint returns UNKNOWN hint for unregistered codes" {
    // audit-error-codes: intentional-fake
    const h = ec.hint("UZ-NONEXISTENT-999");
    try std.testing.expectEqualStrings(ec.UNKNOWN.hint, h);
}
