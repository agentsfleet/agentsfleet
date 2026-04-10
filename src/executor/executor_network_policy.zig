//! Static compile-time network allowlist for the executor sidecar (M16_003 §3.2).
//!
//! Phase 1: bare-metal hosts set EXECUTOR_NETWORK_POLICY=registry_allowlist to
//! permit agent dependency installs (npm, pip, cargo, go get) against public
//! package registries. In Phase 1, --share-net is passed to bwrap and the full
//! host network is accessible; the REGISTRY_ALLOWLIST entries are logged for
//! observability only — TCP-layer restriction to these hosts is Phase 2 (nftables).
//!
//! Phase 2 (out of scope): nftables egress rules restrict traffic to REGISTRY_ALLOWLIST.

/// Public package registries permitted under the `registry_allowlist` policy.
/// This is a compile-time constant — no per-run override path exists.
pub const REGISTRY_ALLOWLIST = [_][]const u8{
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "static.crates.io",
    "crates.io",
    "index.crates.io",
    "proxy.golang.org",
    "sum.golang.org",
};

// ── M3_001: Per-Zombie domain merging ─────────────────────────────────────────

const std = @import("std");
const Allocator = std.mem.Allocator;

/// Merge static registry allowlist with per-Zombie skill domains.
/// Returns a deduplicated slice of all allowed domains. Caller owns.
pub fn mergeAllowlists(
    alloc: Allocator,
    zombie_domains: []const []const u8,
) ![]const []const u8 {
    var seen = std.StringHashMap(void).init(alloc);
    defer seen.deinit();

    var result: std.ArrayList([]const u8) = .{};
    errdefer result.deinit(alloc);

    // Add static registry hosts first
    for (&REGISTRY_ALLOWLIST) |host| {
        try seen.put(host, {});
        try result.append(alloc, host);
    }

    // Add per-Zombie domains, dedup, validate
    for (zombie_domains) |domain| {
        if (!isValidDomain(domain)) return error.InvalidDomain;
        const gop = try seen.getOrPut(domain);
        if (!gop.found_existing) {
            try result.append(alloc, domain);
        }
    }
    return result.toOwnedSlice(alloc);
}

/// Validate a domain string: no injection chars, no whitespace, min length 3.
fn isValidDomain(domain: []const u8) bool {
    if (domain.len < 3) return false;
    var has_dot = false;
    for (domain) |ch| {
        if (std.ascii.isWhitespace(ch)) return false;
        if (ch == ';' or ch == '|' or ch == '&' or ch == '*') return false;
        if (ch == '\n' or ch == '\r' or ch == 0) return false;
        if (ch == '.') has_dot = true;
    }
    return has_dot;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// T1 — Happy path

test "REGISTRY_ALLOWLIST is non-empty" {
    try std.testing.expect(REGISTRY_ALLOWLIST.len > 0);
}

// T2 / T10 — All 8 expected hosts are present and correct

test "REGISTRY_ALLOWLIST contains all 8 expected package registry hosts" {
    // Spec §3.1 lists exactly these hosts. A missing entry means agent dependency
    // installs for the corresponding ecosystem will fail silently on bare-metal.
    const expected = [_][]const u8{
        "registry.npmjs.org",
        "pypi.org",
        "files.pythonhosted.org",
        "static.crates.io",
        "crates.io",
        "index.crates.io",
        "proxy.golang.org",
        "sum.golang.org",
    };
    try std.testing.expectEqual(@as(usize, expected.len), REGISTRY_ALLOWLIST.len);
    for (expected) |want| {
        var found = false;
        for (REGISTRY_ALLOWLIST) |have| {
            if (std.mem.eql(u8, have, want)) {
                found = true;
                break;
            }
        }
        if (!found) {
            std.debug.print("\nREGISTRY_ALLOWLIST missing host: {s}\n", .{want});
        }
        try std.testing.expect(found);
    }
}

// T2 — No duplicate entries

test "REGISTRY_ALLOWLIST has no duplicate hostnames" {
    for (REGISTRY_ALLOWLIST, 0..) |a, i| {
        for (REGISTRY_ALLOWLIST[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a, b)) {
                std.debug.print("\nREGISTRY_ALLOWLIST duplicate entry: {s}\n", .{a});
                try std.testing.expect(false);
            }
        }
    }
}

// T10 — Every entry is a non-empty string with no whitespace

test "REGISTRY_ALLOWLIST entries are non-empty and contain no whitespace" {
    for (REGISTRY_ALLOWLIST) |host| {
        try std.testing.expect(host.len > 0);
        for (host) |ch| {
            try std.testing.expect(!std.ascii.isWhitespace(ch));
        }
    }
}

// T7 — Exact count pinned — catches accidental addition or removal

test "REGISTRY_ALLOWLIST length is exactly 8 (spec §3.1 pinned count)" {
    // If this fails the spec §3.1 list changed — update this test intentionally.
    try std.testing.expectEqual(@as(usize, 8), REGISTRY_ALLOWLIST.len);
}

// ── M3_001: mergeAllowlists tests ──────────────────────────────────────────

test "mergeAllowlists with empty zombie domains returns registry only" {
    const alloc = std.testing.allocator;
    const merged = try mergeAllowlists(alloc, &.{});
    defer alloc.free(merged);
    try std.testing.expectEqual(@as(usize, 8), merged.len);
}

test "mergeAllowlists adds zombie domains without duplicates" {
    const alloc = std.testing.allocator;
    const zombie_domains = &[_][]const u8{ "api.slack.com", "api.github.com" };
    const merged = try mergeAllowlists(alloc, zombie_domains);
    defer alloc.free(merged);
    // 8 registry + 2 new = 10
    try std.testing.expectEqual(@as(usize, 10), merged.len);
}

test "mergeAllowlists deduplicates zombie domains that overlap" {
    const alloc = std.testing.allocator;
    const zombie_domains = &[_][]const u8{ "api.slack.com", "api.slack.com" };
    const merged = try mergeAllowlists(alloc, zombie_domains);
    defer alloc.free(merged);
    // 8 registry + 1 unique new = 9
    try std.testing.expectEqual(@as(usize, 9), merged.len);
}

test "mergeAllowlists rejects injection attempt" {
    const alloc = std.testing.allocator;
    const bad = &[_][]const u8{"evil.com; rm -rf /"};
    try std.testing.expectError(error.InvalidDomain, mergeAllowlists(alloc, bad));
}

test "mergeAllowlists rejects null byte domain" {
    const alloc = std.testing.allocator;
    const bad = &[_][]const u8{"evil\x00.com"};
    try std.testing.expectError(error.InvalidDomain, mergeAllowlists(alloc, bad));
}

test "mergeAllowlists rejects too-short domain" {
    const alloc = std.testing.allocator;
    const bad = &[_][]const u8{"ab"};
    try std.testing.expectError(error.InvalidDomain, mergeAllowlists(alloc, bad));
}

test "isValidDomain accepts normal domains" {
    try std.testing.expect(isValidDomain("api.slack.com"));
    try std.testing.expect(isValidDomain("github.com"));
}

test "isValidDomain rejects whitespace and metacharacters" {
    try std.testing.expect(!isValidDomain("evil .com"));
    try std.testing.expect(!isValidDomain("evil|com"));
    try std.testing.expect(!isValidDomain("evil&com"));
    try std.testing.expect(!isValidDomain(""));
}

test "isValidDomain rejects wildcards and dotless strings" {
    try std.testing.expect(!isValidDomain("*.evil.com"));
    try std.testing.expect(!isValidDomain("localhost"));
    try std.testing.expect(!isValidDomain("no-dot-here"));
}

test "isValidDomain allows IP addresses with dots" {
    // IPs pass the dot check — nftables Phase 2 handles IP restriction
    try std.testing.expect(isValidDomain("1.2.3.4"));
}
