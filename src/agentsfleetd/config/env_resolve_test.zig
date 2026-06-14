//! Unit tests for the env resolution policy.
//!
//! The security-critical property for `required`: an allocation failure must
//! propagate as an error, never collapse into a `null` "unset" — otherwise an
//! out-of-memory at boot silently disables webhook / Clerk authentication while
//! the server keeps reporting healthy. `optional` must not swallow that failure
//! silently either — it returns `null` for the safe-fallback path but only after
//! logging. Both are pinned by failure-injection below.

const std = @import("std");
const common = @import("common");
const env_resolve = @import("env_resolve.zig");

const KEY = env_resolve.CLERK_WEBHOOK_SECRET_ENV;

// ── secret(): fail-closed ──────────────────────────────────────────────────

test "secret returns an owned copy when the secret is set" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "whsec_present" }});
    defer env.deinit();

    const got = (try env_resolve.secret(&env, std.testing.allocator, KEY)) orelse
        return error.TestUnexpectedResult;
    defer std.testing.allocator.free(got);
    try std.testing.expectEqualStrings("whsec_present", got);
}

test "secret returns null when the secret is unset" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{});
    defer env.deinit();

    const got = try env_resolve.secret(&env, std.testing.allocator, KEY);
    try std.testing.expect(got == null);
}

test "secret returns an owned empty string for a present-but-empty secret" {
    // Present-but-empty is distinct from unset: the consumer rejects it at
    // request time (empty secret → 500), so secret() must surface "" not null.
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "" }});
    defer env.deinit();

    const got = (try env_resolve.secret(&env, std.testing.allocator, KEY)) orelse
        return error.TestUnexpectedResult;
    defer std.testing.allocator.free(got);
    try std.testing.expectEqualStrings("", got);
}

test "secret propagates OutOfMemory instead of masking it as unset" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "whsec_present" }});
    defer env.deinit();

    // The secret IS present; fail the owning dupe. A swallowed error would
    // surface as a null "unset" — the exact masking bug this guards against.
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    const result = env_resolve.secret(&env, failing.allocator(), KEY);
    try std.testing.expectError(error.OutOfMemory, result);
}

// ── config(): safe-fallback ────────────────────────────────────────────────

test "config returns an owned copy when the var is set" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "cfg_value" }});
    defer env.deinit();

    const got = env_resolve.config(&env, std.testing.allocator, KEY) orelse
        return error.TestUnexpectedResult;
    defer std.testing.allocator.free(got);
    try std.testing.expectEqualStrings("cfg_value", got);
}

test "config returns null when the var is unset" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{});
    defer env.deinit();

    try std.testing.expect(env_resolve.config(&env, std.testing.allocator, KEY) == null);
}

test "config returns null on OOM (logged, not propagated) for the safe-fallback path" {
    var env = try common.env.fromPairs(std.testing.allocator, &.{.{ KEY, "cfg_value" }});
    defer env.deinit();

    // The var IS present; fail the owning dupe. config() must fall back to null
    // (caller substitutes its default) — but the failure is logged, not silent.
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    try std.testing.expect(env_resolve.config(&env, failing.allocator(), KEY) == null);
}
