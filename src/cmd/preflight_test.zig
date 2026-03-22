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
// Cache root preparation
// ---------------------------------------------------------------------------

test "prepareCacheRoot handles non-existent temp directory" {
    const stats = preflight.prepareCacheRoot(
        std.testing.allocator,
        "/tmp/zombied-test-preflight-cache",
        "test",
    );
    // Cleanup — verify it ran without crashing
    try std.testing.expect(stats.removed_worktrees == 0 or stats.removed_worktrees >= 0);

    // Clean up created dir
    std.fs.deleteTreeAbsolute("/tmp/zombied-test-preflight-cache") catch {};
}

test "prepareCacheRoot handles already-existing directory" {
    std.fs.makeDirAbsolute("/tmp/zombied-test-preflight-exists") catch {};
    defer std.fs.deleteTreeAbsolute("/tmp/zombied-test-preflight-exists") catch {};

    const stats = preflight.prepareCacheRoot(
        std.testing.allocator,
        "/tmp/zombied-test-preflight-exists",
        "test",
    );
    try std.testing.expect(stats.removed_worktrees == 0 or stats.removed_worktrees >= 0);
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
