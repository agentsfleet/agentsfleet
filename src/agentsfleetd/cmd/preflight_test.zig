const std = @import("std");
const constants = @import("common");
const preflight = @import("preflight.zig");

// `initPostHog` and `parseMigrateOnStart` read an injected `common.env.Map` rather
// than the process environment, so each test builds the exact environment it means.
// Mutating the real environment would leak across tests sharing this process.

// ---------------------------------------------------------------------------
// PostHog init
// ---------------------------------------------------------------------------

test "initPostHog returns null client when POSTHOG_API_KEY is unset" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{});
    defer env.deinit();

    const result = preflight.initPostHog(&env, alloc);
    defer result.deinit(alloc);

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
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "1" }});
    defer env.deinit();
    try std.testing.expect(try preflight.parseMigrateOnStart(&env, alloc));
}

test "parseMigrateOnStart returns false for '0'" {
    const alloc = std.testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "0" }});
    defer env.deinit();
    try std.testing.expect(!try preflight.parseMigrateOnStart(&env, alloc));
}

// ---------------------------------------------------------------------------
// Signal handlers
// ---------------------------------------------------------------------------

var test_signal_received = std.atomic.Value(bool).init(false);

fn testSignalHandler(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    test_signal_received.store(true, .release);
}

test "installSignalHandlers routes a delivered INT to the given handler" {
    // The suite runs in this process: leaving our handler installed would swallow
    // a real Ctrl-C for every test that follows, so the previous actions are
    // restored before returning.
    var prev_int: std.posix.Sigaction = undefined;
    var prev_term: std.posix.Sigaction = undefined;
    std.posix.sigaction(std.posix.SIG.INT, null, &prev_int);
    std.posix.sigaction(std.posix.SIG.TERM, null, &prev_term);
    defer {
        std.posix.sigaction(std.posix.SIG.INT, &prev_int, null);
        std.posix.sigaction(std.posix.SIG.TERM, &prev_term, null);
    }
    test_signal_received.store(false, .release);

    preflight.installSignalHandlers(testSignalHandler);

    var installed_int: std.posix.Sigaction = undefined;
    var installed_term: std.posix.Sigaction = undefined;
    std.posix.sigaction(std.posix.SIG.INT, null, &installed_int);
    std.posix.sigaction(std.posix.SIG.TERM, null, &installed_term);

    const expected = @intFromPtr(&testSignalHandler);
    try std.testing.expectEqual(expected, @intFromPtr(installed_int.handler.handler.?));
    try std.testing.expectEqual(expected, @intFromPtr(installed_term.handler.handler.?));

    // Raise only AFTER the handler is proven installed: on the default action a
    // delivered INT would terminate the test runner instead of failing this test.
    try std.posix.raise(std.posix.SIG.INT);
    try std.testing.expect(test_signal_received.load(.acquire));
}
