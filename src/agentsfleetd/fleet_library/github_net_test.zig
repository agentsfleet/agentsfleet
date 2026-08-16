//! Unit tests for the GitHub fetch network/SSRF layer (`github_net.zig`). The
//! pure guards — host allowlist and the capped drain — are tested here against
//! internal/link-local/look-alike hosts and over-cap input, along with every
//! refusal `download` reaches before it opens a socket.
//!
//! What stays uncovered here is only what needs the wire: once a URL clears the
//! scheme and host guards, the request, the redirect hop and the capped body
//! read all need an allowlisted GitHub host, so the allowlist that makes this
//! module safe is the same thing that puts those lines out of a unit test's
//! reach. They are covered by integration + the adversarial red-team.

const std = @import("std");
const common = @import("common");
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

test "download refuses every URL the guards reject before it opens a socket" {
    // `download` itself was never called: the guards were proven only through
    // the pure predicates, so nothing asserted that the entry point consults
    // them at all. Each of these fails inside the first hop, before any
    // connection attempt, which is what makes them safe to run offline — a
    // guard that regressed into connecting first would hang or fail here
    // instead of quietly opening the socket.
    const alloc = testing.allocator;
    const io = common.globalIo();

    // Not a URI at all.
    try testing.expectError(
        github_net.NetError.InvalidUrl,
        github_net.download(alloc, io, "not-a-url"),
    );
    // Parses, but plaintext — refused before the host is even considered, so a
    // downgrade cannot be smuggled past the allowlist.
    try testing.expectError(
        github_net.NetError.DisallowedRedirect,
        github_net.download(alloc, io, "http://api.github.com/repos/o/r/tarball/main"),
    );
    // HTTPS to the link-local metadata address — the Server-Side Request
    // Forgery (SSRF) target the allowlist exists for.
    try testing.expectError(
        github_net.NetError.DisallowedRedirect,
        github_net.download(alloc, io, "https://169.254.169.254/latest/meta-data"),
    );
    // A look-alike that only prefixes an allowed host.
    try testing.expectError(
        github_net.NetError.DisallowedRedirect,
        github_net.download(alloc, io, "https://api.github.com.evil.com/x"),
    );
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
