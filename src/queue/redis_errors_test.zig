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
    try std.testing.expect(!redis_errors.isResumable(error.WriteFailed));
    try std.testing.expect(!redis_errors.isResumable(error.RedisProtocolDesync));
}

test "RedisError surface stays at 8 variants" {
    // pin test: literal is the contract. Growing the set requires
    // amending the resumable / non-resumable classification — adding
    // an error name alone is not enough; the spec table updates too.
    const variants = @typeInfo(redis_errors.RedisError).error_set.?;
    try std.testing.expectEqual(@as(usize, 8), variants.len);
}
