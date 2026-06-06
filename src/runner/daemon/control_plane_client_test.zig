//! Unit tests for the control-plane client's `/renew` status mapping: the pure
//! `classifyRenew` (HTTP status + body → RenewResult) and the
//! `isTerminalRenewStatus` classifier. No HTTP — the (status, body) pairs stand
//! in for server responses, so the fail-safe contract (2xx renews, a definitive
//! 4xx terminates, every other status retries) is asserted directly.
//!
//! pin test: the HTTP status codes are the contract this maps, kept as literals.

const std = @import("std");
const testing = std.testing;
const client = @import("control_plane_client.zig");

test "classifyRenew: a 2xx parses the new kill deadline into renewed" {
    const out = try client.classifyRenew(testing.allocator, 200, "{\"lease_expires_at\":1900000000123}");
    try testing.expectEqual(client.RenewResult{ .renewed = 1_900_000_000_123 }, out);
}

test "classifyRenew: a 2xx with an unparseable body is a malformed response" {
    try testing.expectError(error.MalformedResponse, client.classifyRenew(testing.allocator, 200, "{not json"));
}

test "classifyRenew: each terminal 4xx maps to terminal carrying that status" {
    inline for (.{ 401, 402, 404, 409 }) |status| {
        const out = try client.classifyRenew(testing.allocator, status, "");
        try testing.expectEqual(client.RenewResult{ .terminal = status }, out);
    }
}

test "classifyRenew: non-terminal 4xx and all 5xx are retryable BadStatus" {
    inline for (.{ 400, 403, 408, 429, 500, 503 }) |status| {
        try testing.expectError(error.BadStatus, client.classifyRenew(testing.allocator, status, ""));
    }
}

test "isTerminalRenewStatus: only 401/402/404/409 are terminal" {
    inline for (.{ 401, 402, 404, 409 }) |s| try testing.expect(client.isTerminalRenewStatus(s));
    inline for (.{ 200, 400, 403, 408, 410, 429, 500, 503 }) |s| try testing.expect(!client.isTerminalRenewStatus(s));
}

test "the control-plane client holds no persistent file descriptor" {
    // fd-stateless by construction: every credential-bearing socket is opened
    // per call via std.http.Client and freed before the verb returns, so none is
    // live when the supervisor forks the sandboxed child. The struct is exactly
    // { base_url, io }; a future pooled/persistent socket field would be an
    // inheritable credential vector and must be reviewed before it lands.
    const fields = @typeInfo(client).@"struct".fields;
    try testing.expectEqual(@as(usize, 2), fields.len);
    inline for (fields) |f| {
        const known = comptime (std.mem.eql(u8, f.name, "base_url") or std.mem.eql(u8, f.name, "io"));
        if (!known)
            @compileError("control-plane client gained field '" ++ f.name ++ "' — review for fd-statelessness (no persistent credential socket)");
    }
}
