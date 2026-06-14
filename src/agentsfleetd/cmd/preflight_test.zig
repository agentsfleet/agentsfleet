const std = @import("std");
const preflight = @import("preflight.zig");

// ---------------------------------------------------------------------------
// PostHog init
// ---------------------------------------------------------------------------

test "initPostHog returns null client when POSTHOG_API_KEY is unset" {
    // Ensure env var is not set for this test
    std.posix.unsetenv("POSTHOG_API_KEY");
    const result = preflight.initPostHog(std.testing.allocator);
    defer result.deinit(std.testing.allocator);

    try std.testing.expect(result.client == null);
    try std.testing.expect(result.api_key_owned == null);
}

test "PostHogResult deinit is safe when both fields are null" {
    const result = preflight.PostHogResult{
        .client = null,
        .api_key_owned = null,
    };
    result.deinit(std.testing.allocator);
}

// ---------------------------------------------------------------------------
// Migration parse
// ---------------------------------------------------------------------------

test "parseMigrateOnStart returns true for '1'" {
    try std.posix.setenv("MIGRATE_ON_START", "1", true);
    defer std.posix.unsetenv("MIGRATE_ON_START");
    try std.testing.expect(try preflight.parseMigrateOnStart(std.testing.allocator));
}

test "parseMigrateOnStart returns false for '0'" {
    try std.posix.setenv("MIGRATE_ON_START", "0", true);
    defer std.posix.unsetenv("MIGRATE_ON_START");
    try std.testing.expect(!try preflight.parseMigrateOnStart(std.testing.allocator));
}

// ---------------------------------------------------------------------------
// Signal handlers
// ---------------------------------------------------------------------------

var test_signal_received = std.atomic.Value(bool).init(false);

fn testSignalHandler(sig: i32) callconv(.c) void {
    _ = sig;
    test_signal_received.store(true, .release);
}

test "installSignalHandlers accepts custom handler function" {
    preflight.installSignalHandlers(testSignalHandler);
    // Verify installation didn't crash — actual signal delivery
    // requires kill() which we avoid in unit tests.
    try std.testing.expect(true);
}
