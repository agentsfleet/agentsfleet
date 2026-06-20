//! Unit tests for the GitHub fetch network/SSRF layer (`github_net.zig`). The
//! pure guards — host allowlist and the capped drain — are tested here against
//! internal/link-local/look-alike hosts and over-cap input. The socket path
//! (`download`/`fetchOnce`) needs GitHub and is covered by integration + the
//! adversarial red-team.

const std = @import("std");
const testing = std.testing;
const github_net = @import("github_net.zig");

test "isAllowedHost allows only the GitHub tarball hosts" {
    try testing.expect(github_net.isAllowedHost("api.github.com"));
    try testing.expect(github_net.isAllowedHost("API.GitHub.com"));
    try testing.expect(github_net.isAllowedHost("codeload.github.com"));
    // github.com itself is not a tarball host; everything else is rejected.
    try testing.expect(!github_net.isAllowedHost("github.com"));
    try testing.expect(!github_net.isAllowedHost("169.254.169.254"));
    try testing.expect(!github_net.isAllowedHost("localhost"));
    try testing.expect(!github_net.isAllowedHost("metadata.google.internal"));
    try testing.expect(!github_net.isAllowedHost("api.github.com.evil.com"));
    try testing.expect(!github_net.isAllowedHost("evil.com"));
    try testing.expect(!github_net.isAllowedHost(""));
}

test "drainCapped returns the body when under the cap" {
    const alloc = testing.allocator;
    var r = std.Io.Reader.fixed("hello");
    const out = try github_net.drainCapped(alloc, &r, 100);
    defer alloc.free(out);
    try testing.expectEqualStrings("hello", out);
}

test "drainCapped rejects input over the cap" {
    const alloc = testing.allocator;
    var r = std.Io.Reader.fixed("0123456789abcdef");
    try testing.expectError(error.TooLarge, github_net.drainCapped(alloc, &r, 10));
}

test "isUriHostAllowed accepts tarball hosts and rejects the rest without panicking" {
    // An over-long host (>255 bytes) must be rejected up front, never reach
    // std.Uri.getHost (which would panic on the oversized decode).
    const long = "https://" ++ ("a" ** 300) ++ "/x";
    try testing.expect(!github_net.isUriHostAllowed(try std.Uri.parse(long)));
    try testing.expect(github_net.isUriHostAllowed(try std.Uri.parse("https://api.github.com/repos/o/r/tarball/main")));
    try testing.expect(github_net.isUriHostAllowed(try std.Uri.parse("https://codeload.github.com/o/r/tar.gz/main")));
    try testing.expect(!github_net.isUriHostAllowed(try std.Uri.parse("https://169.254.169.254/latest/meta-data")));
    try testing.expect(!github_net.isUriHostAllowed(try std.Uri.parse("https://evil.com/x")));
}
