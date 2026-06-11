//! Tests for `redis_errors.RedisError` + `isResumable`.
//!
//! The compile-time exhaustive switch in `isResumable` is the primary
//! safeguard against drift; these runtime tests are the belt-and-braces
//! evidence that the predicate's classification matches the documented
//! resumable / non-resumable split.

const std = @import("std");
const redis_errors = @import("redis_errors.zig");

test "isResumable: server-side variants are resumable" {
    try std.testing.expect(redis_errors.isResumable(error.RedisCommandError));
    try std.testing.expect(redis_errors.isResumable(error.RedisXaddFailed));
    try std.testing.expect(redis_errors.isResumable(error.RedisXackFailed));
}

test "isResumable: transport-level variants are not resumable" {
    try std.testing.expect(!redis_errors.isResumable(error.BrokenPipe));
    try std.testing.expect(!redis_errors.isResumable(error.ConnectionResetByPeer));
    try std.testing.expect(!redis_errors.isResumable(error.ReadFailed));
    try std.testing.expect(!redis_errors.isResumable(error.RedisRequestTimeout));
    try std.testing.expect(!redis_errors.isResumable(error.WriteFailed));
    try std.testing.expect(!redis_errors.isResumable(error.RedisProtocolDesync));
}

test "RedisError surface stays at 9 variants" {
    // pin test: literal is the contract. Growing the set requires
    // amending the resumable / non-resumable classification — adding
    // an error name alone is not enough; the spec table updates too.
    const variants = @typeInfo(redis_errors.RedisError).error_set.?;
    try std.testing.expectEqual(@as(usize, 9), variants.len);
}

// ── Env-parser surface (#16) ────────────────────────────────────────────
//
// The boot-path knob `REDIS_REQUEST_TIMEOUT_MS` is parsed via the pure
// helper `parseRequestTimeoutMs` in redis_config.zig. Serve's
// `readRedisRequestTimeoutMs` wraps it with env-read + log-and-exit;
// the test exercises the helper directly so we never depend on env
// state or `std.process.exit`. The env-var-name pin test below ties
// the typed error to the operator-facing identifier — renaming the
// env knob requires a coordinated diff that flips both.

const redis_config = @import("redis_config.zig");

test "parseRequestTimeoutMs: non-numeric raw input surfaces InvalidRequestTimeout" {
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs("abc"));
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs(""));
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs("-1"));
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs("3.14"));
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs("  5000  "));
}

test "parseRequestTimeoutMs: valid integer parses cleanly" {
    try std.testing.expectEqual(@as(u32, 5000), try redis_config.parseRequestTimeoutMs("5000"));
    try std.testing.expectEqual(@as(u32, 0), try redis_config.parseRequestTimeoutMs("0"));
    try std.testing.expectEqual(@as(u32, std.math.maxInt(u32)), try redis_config.parseRequestTimeoutMs("4294967295"));
}

test "parseRequestTimeoutMs: u32 overflow surfaces InvalidRequestTimeout" {
    // 2^32 = 4294967296 — one over the u32 ceiling.
    try std.testing.expectError(error.InvalidRequestTimeout, redis_config.parseRequestTimeoutMs("4294967296"));
}

test "REDIS_REQUEST_TIMEOUT_MS_ENV is the contract scrapers + ops runbooks depend on" {
    // pin test: literal is the contract — operator-facing env-var name.
    // The serve.zig boot log at error path interpolates this const into
    // the surfaced message: a rename here must thread through ops docs
    // (docs.usezombie.com env reference) and any compose / k8s overlays
    // shipping the var to production.
    try std.testing.expectEqualStrings("REDIS_REQUEST_TIMEOUT_MS", redis_config.REDIS_REQUEST_TIMEOUT_MS_ENV);
    try std.testing.expectEqual(@as(u32, 5000), redis_config.REDIS_REQUEST_TIMEOUT_MS_DEFAULT);
}
