//! Edge-case tests for the control-plane client's `/renew` status mapping
//! (`classifyRenew`). Mirrors the pure (status, body) → RenewResult contract of
//! the sibling unit test, pinning two boundaries: a 2xx whose body lacks the
//! required `lease_expires_at` is malformed, and 410 Gone is NOT one of the four
//! terminal statuses — it is retryable like any other non-terminal 4xx. No HTTP;
//! the (status, body) pairs stand in for server responses.
//!
//! pin test: the HTTP status codes are the contract this maps, kept as literals.

const std = @import("std");
const testing = std.testing;
const client = @import("control_plane_client.zig");

test "classifyRenew should return MalformedResponse for a 2xx with an empty body" {
    // lease_expires_at is required; an empty body cannot parse → malformed, not a
    // silent zero deadline.
    try testing.expectError(error.MalformedResponse, client.classifyRenew(testing.allocator, 200, ""));
}

test "classifyRenew should treat 410 Gone as retryable BadStatus, not terminal" {
    // Only 401/402/404/409 are definitive. 410 is a non-terminal 4xx → BadStatus
    // so the caller retries on the next tick rather than killing a healthy run.
    try testing.expectError(error.BadStatus, client.classifyRenew(testing.allocator, 410, ""));
    try testing.expect(!client.isTerminalRenewStatus(410));
}
