//! Network + Server-Side Request Forgery (SSRF) layer for GitHub Fleet Bundle
//! fetch. Stateless on purpose — `download` is the only entry point and is the
//! daemon's single outbound seam to GitHub. Two SSRF guards live here and
//! nowhere else:
//!   1. Host allowlist on every hop. The first GET targets a caller-built URL
//!      whose host must be `api.github.com`; the tarball endpoint then 302s, and
//!      the redirect target is re-validated against `ALLOWED_HOSTS` before we
//!      connect. Redirects are NOT auto-followed (`redirect_behavior =
//!      .unhandled`) so every `Location` is inspected first.
//!   2. One redirect hop only, HyperText Transfer Protocol Secure (HTTPS) only.
//!      A second redirect, a non-HTTPS scheme, or a host outside the allowlist
//!      is rejected, so a compromised or open-redirecting upstream cannot bounce
//!      us onto an internal address (link-local metadata, RFC1918, localhost).
//! Host comparison runs against the percent-decoded host (`getHost`), so encoded
//! look-alikes cannot slip a disallowed host past the allowlist. A compressed-
//! size cap bounds the download before it is buffered; the decompression-bomb and
//! path-traversal guards live in `github_source.zig`.
//!
//! Timeout posture: `std.http.Client` exposes only a connect timeout, no body-read
//! deadline, so a hung/slowloris upstream cannot be aborted mid-read by any std
//! mechanism. Triggering it requires a TLS man-in-the-middle of an allowlist host
//! (api/codeload.github.com), and the blast radius is bounded by the import
//! concurrency cap in the import handler: a stalled fetch holds at most one of N
//! import slots (reclaimed by the OS TCP timeout), never the general request pool.
//! A true per-read deadline awaits a `std.http.Client` capability.

const std = @import("std");

const HostName = std.Io.net.HostName;

pub const NetError = error{
    InvalidUrl,
    FetchFailed,
    DisallowedRedirect,
    TarballTooLarge,
};

/// Domain-neutral failure for `drainCapped`; callers map it to their own set.
pub const DrainError = error{ ReadFailed, TooLarge };

/// Compressed `.tar.gz` download ceiling — bounds bytes buffered before the
/// decompression-bomb guard in `github_source.zig` runs.
pub const MAX_COMPRESSED_TARBALL: usize = 8 * 1024 * 1024;

const IO_CHUNK_LEN: usize = 64 * 1024;
const REDIRECT_BUF_LEN: usize = 8 * 1024;
const HTTP_2XX_FAMILY: u16 = 100; // status / HTTP_2XX_FAMILY == 2 → 2xx
const USER_AGENT = "agentsfleetd"; // GitHub API 403s requests with no User-Agent

const API_HOST = "api.github.com";
const CODELOAD_HOST = "codeload.github.com";
const ALLOWED_HOSTS = [_][]const u8{ API_HOST, CODELOAD_HOST };
const SCHEME_HTTPS = "https";

const Hop = union(enum) { body: []u8, redirect: []u8 };

/// Fetch the raw `.tar.gz` at `url` (HTTPS, host in `ALLOWED_HOSTS`). Follows at
/// most one redirect, re-validating the target host. Caller owns the result.
pub fn download(alloc: std.mem.Allocator, io: std.Io, url: []const u8) (NetError || std.mem.Allocator.Error)![]u8 {
    var client: std.http.Client = .{ .allocator = alloc, .io = io };
    defer client.deinit();

    switch (try fetchOnce(alloc, &client, url)) {
        .body => |b| return b,
        .redirect => |loc| {
            defer alloc.free(loc);
            switch (try fetchOnce(alloc, &client, loc)) {
                .body => |b| return b,
                .redirect => |loc2| {
                    alloc.free(loc2);
                    return NetError.DisallowedRedirect; // one hop only
                },
            }
        },
    }
}

/// `host` ∈ {api.github.com, codeload.github.com}, case-insensitive, compared
/// against the percent-decoded host. Pure; unit-tested against internal,
/// link-local, and look-alike hosts.
pub fn isAllowedHost(host: []const u8) bool {
    for (ALLOWED_HOSTS) |allowed| {
        if (std.ascii.eqlIgnoreCase(host, allowed)) return true;
    }
    return false;
}

/// Validate a parsed URI's host against the allowlist WITHOUT tripping the
/// `std.Uri.getHost` assertion. `std.Uri.parse` does not length-check the host,
/// and `getHost` asserts the decoded host fits a `HostName.max_len` (255-byte)
/// buffer — an adversarial redirect Location whose host decodes to >255 bytes
/// would panic the daemon. Percent-decoding never grows a string, so an encoded
/// length <= max guarantees the decode fits; reject anything larger up front.
pub fn isUriHostAllowed(uri: std.Uri) bool {
    const comp = uri.host orelse return false;
    const encoded_len = switch (comp) {
        .raw => |s| s.len,
        .percent_encoded => |s| s.len,
    };
    if (encoded_len > HostName.max_len) return false;
    var host_buf: [HostName.max_len]u8 = undefined;
    const host = uri.getHost(&host_buf) catch return false;
    return isAllowedHost(host.bytes);
}

fn fetchOnce(alloc: std.mem.Allocator, client: *std.http.Client, url: []const u8) (NetError || std.mem.Allocator.Error)!Hop {
    const uri = std.Uri.parse(url) catch return NetError.InvalidUrl;
    if (!std.ascii.eqlIgnoreCase(uri.scheme, SCHEME_HTTPS)) return NetError.DisallowedRedirect;
    if (!isUriHostAllowed(uri)) return NetError.DisallowedRedirect;

    const headers = [_]std.http.Header{.{ .name = "user-agent", .value = USER_AGENT }};
    var req = client.request(.GET, uri, .{
        .redirect_behavior = .unhandled,
        .extra_headers = &headers,
    }) catch return NetError.FetchFailed;
    defer req.deinit();
    req.sendBodiless() catch return NetError.FetchFailed;

    var redirect_buf: [REDIRECT_BUF_LEN]u8 = undefined;
    var resp = req.receiveHead(&redirect_buf) catch return NetError.FetchFailed;
    if (resp.head.status.class() == .redirect) {
        const loc = resp.head.location orelse return NetError.FetchFailed;
        return .{ .redirect = try alloc.dupe(u8, loc) };
    }
    if (@intFromEnum(resp.head.status) / HTTP_2XX_FAMILY != 2) return NetError.FetchFailed;

    var transfer_buf: [IO_CHUNK_LEN]u8 = undefined;
    const body_reader = resp.reader(&transfer_buf);
    const body = drainCapped(alloc, body_reader, MAX_COMPRESSED_TARBALL) catch |e| switch (e) {
        error.TooLarge => return NetError.TarballTooLarge,
        error.ReadFailed => return NetError.FetchFailed,
        error.OutOfMemory => return error.OutOfMemory,
    };
    return .{ .body = body };
}

/// Drain `r` into an owned slice, failing at `max` bytes. The single audited
/// capped-read shared by the HTTP body download and the gzip decompression
/// stream (`github_source.gunzipCapped`) — one bounded read to verify, not two.
pub fn drainCapped(alloc: std.mem.Allocator, r: *std.Io.Reader, max: usize) (DrainError || std.mem.Allocator.Error)![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    // Reserve the exact ceiling once: accumulation then cannot overshoot the cap
    // through ArrayList geometric growth, so peak transient is ~max rather than the
    // ~2.5x a growing buffer reaches near the cap. Cold import path — the
    // reservation is freed as soon as the bundle is consumed and is bounded by the
    // import concurrency cap.
    try out.ensureTotalCapacityPrecise(alloc, max + 1);
    var buf: [IO_CHUNK_LEN]u8 = undefined;
    while (true) {
        const n = r.readSliceShort(&buf) catch return DrainError.ReadFailed;
        if (n == 0) break;
        if (out.items.len + n > max) return DrainError.TooLarge;
        out.appendSliceAssumeCapacity(buf[0..n]);
    }
    return out.toOwnedSlice(alloc);
}

test {
    _ = @import("github_net_test.zig");
}
