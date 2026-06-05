//! Unit tests for boot-time secret resolution.
//!
//! The security-critical property: an allocation failure must propagate as an
//! error, never collapse into a `null` "unset" — otherwise an out-of-memory at
//! boot silently disables webhook / Clerk authentication while the server keeps
//! reporting healthy. The failure-injection test below pins that contract.

const std = @import("std");
const common = @import("common");
const boot_secrets = @import("serve_boot_secrets.zig");

const KEY = boot_secrets.CLERK_WEBHOOK_SECRET_ENV;

test "resolve returns an owned copy when the secret is set" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "whsec_present" }});
    defer env.deinit();

    const got = (try boot_secrets.resolve(&env, std.testing.allocator, KEY)) orelse
        return error.TestUnexpectedResult;
    defer std.testing.allocator.free(got);
    try std.testing.expectEqualStrings("whsec_present", got);
}

test "resolve returns null when the secret is unset" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{});
    defer env.deinit();

    const got = try boot_secrets.resolve(&env, std.testing.allocator, KEY);
    try std.testing.expect(got == null);
}

test "resolve returns an owned empty string for a present-but-empty secret" {
    // Present-but-empty is distinct from unset: the consumer rejects it at
    // request time (empty secret → 500), so resolve must surface "" not null.
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "" }});
    defer env.deinit();

    const got = (try boot_secrets.resolve(&env, std.testing.allocator, KEY)) orelse
        return error.TestUnexpectedResult;
    defer std.testing.allocator.free(got);
    try std.testing.expectEqualStrings("", got);
}

test "resolve propagates OutOfMemory instead of masking it as unset" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "whsec_present" }});
    defer env.deinit();

    // The secret IS present; fail the owning dupe. A swallowed error would
    // surface as a null "unset" — the exact masking bug this guards against.
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    const result = boot_secrets.resolve(&env, failing.allocator(), KEY);
    try std.testing.expectError(error.OutOfMemory, result);
}
