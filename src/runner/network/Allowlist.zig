//! Allowlist.zig — the merged egress allowlist for one lease.
//!
//! The single source the kernel layer (nftables) and the L7 tool checks
//! (`http_request`/`web_fetch`) both consume: the operator-fed registry baseline
//! ∪ the per-zombie `network.allow` ∪ the lease's inference host. Deduped
//! first-seen, validated, fail-closed on a malformed entry. File-as-struct
//! (`@This()`), std idiom.
//!
//! The registry baseline is fed from OUTSIDE — `RUNNER_REGISTRY_ALLOWLIST` →
//! `daemon/config` → `build(registry, …)`. It is never a compile-time source;
//! `DEFAULT_REGISTRY` is the fallback the daemon substitutes only when the
//! operator sets nothing.

const Allowlist = @This();

/// Merged, deduped, validated hostnames (first-seen order). Owned by `alloc`.
names: []const []const u8,
alloc: std.mem.Allocator,

/// Fallback registry baseline — the daemon feeds this in when
/// `RUNNER_REGISTRY_ALLOWLIST` is unset. NOT the authoritative source: the
/// operator overrides it from outside. Single-sourced here (RULE UFS).
pub const DEFAULT_REGISTRY = [_][]const u8{
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "static.crates.io",
    "crates.io",
    "index.crates.io",
    "proxy.golang.org",
    "sum.golang.org",
};

pub const Error = error{ InvalidDomain, OutOfMemory };

/// Build the merged allowlist: `inference_host` (skipped when empty) ∪
/// `registry` (operator-fed) ∪ `network_allow` (per-zombie), deduped in
/// first-seen order. Every entry is validated; a malformed one fails closed with
/// `error.InvalidDomain` rather than silently widening egress. Caller owns the
/// result — call `deinit`.
pub fn build(
    alloc: std.mem.Allocator,
    registry: []const []const u8,
    network_allow: []const []const u8,
    inference_host: []const u8,
) Error!Allowlist {
    var seen = std.StringHashMap(void).init(alloc);
    defer seen.deinit();

    var names: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (names.items) |n| alloc.free(n);
        names.deinit(alloc);
    }

    // Inference host first — load-bearing: the agent's LLM call must reach it or
    // the whole lease is dead, so it is never dropped by a too-tight list.
    if (inference_host.len > 0) try appendUnique(alloc, &names, &seen, inference_host);
    for (registry) |h| try appendUnique(alloc, &names, &seen, h);
    for (network_allow) |h| try appendUnique(alloc, &names, &seen, h);

    return .{ .names = try names.toOwnedSlice(alloc), .alloc = alloc };
}

/// Exact-hostname membership — the one predicate the L7 tool checks and the nft
/// installer agree on (closes the historical split-brain).
pub fn contains(self: Allowlist, host: []const u8) bool {
    for (self.names) |n| if (std.mem.eql(u8, n, host)) return true;
    return false;
}

pub fn deinit(self: *Allowlist) void {
    for (self.names) |n| self.alloc.free(n);
    self.alloc.free(self.names);
    self.names = &.{};
}

fn appendUnique(
    alloc: std.mem.Allocator,
    names: *std.ArrayList([]const u8),
    seen: *std.StringHashMap(void),
    host: []const u8,
) Error!void {
    if (!isValidDomain(host)) return error.InvalidDomain;
    const gop = try seen.getOrPut(host);
    if (gop.found_existing) return;
    const owned = try alloc.dupe(u8, host);
    errdefer alloc.free(owned);
    try names.append(alloc, owned);
}

/// Domain shape guard (migrated from the retired `runner_network_policy`): min
/// length 3, at least one dot, no whitespace, no shell/injection metacharacters.
/// IPv4 literals pass (dotted) — the kernel layer enforces IP membership.
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

// ── Tests ───────────────────────────────────────────────────────────────────

test "build merges registry, network_allow, inference; deduped; inference first" {
    const a = std.testing.allocator;
    var al = try build(
        a,
        &.{ "registry.npmjs.org", "pypi.org" },
        &.{ "api.github.com", "pypi.org" }, // pypi dups the registry entry
        "api.fireworks.ai",
    );
    defer al.deinit();
    try std.testing.expectEqualStrings("api.fireworks.ai", al.names[0]); // inference first
    try std.testing.expectEqual(@as(usize, 4), al.names.len); // fireworks, npmjs, pypi, github
    try std.testing.expect(al.contains("api.github.com"));
    try std.testing.expect(al.contains("api.fireworks.ai"));
    try std.testing.expect(!al.contains("evil.com"));
}

test "DEFAULT_REGISTRY pins the 8 package-registry hosts (fallback only)" {
    try std.testing.expectEqual(@as(usize, 8), DEFAULT_REGISTRY.len);
}

test "build skips an empty inference host" {
    const a = std.testing.allocator;
    var al = try build(a, &.{"crates.io"}, &.{}, "");
    defer al.deinit();
    try std.testing.expectEqual(@as(usize, 1), al.names.len);
    try std.testing.expectEqualStrings("crates.io", al.names[0]);
}

test "registry is operator-fed, not a hardcoded baseline" {
    const a = std.testing.allocator;
    var al = try build(a, &.{"my.private.registry"}, &.{}, "");
    defer al.deinit();
    try std.testing.expect(al.contains("my.private.registry"));
    // The DEFAULT is NOT auto-included — the caller supplies the registry.
    try std.testing.expect(!al.contains("registry.npmjs.org"));
}

test "build fails closed on a malformed / injection-shaped domain" {
    const a = std.testing.allocator;
    try std.testing.expectError(error.InvalidDomain, build(a, &.{}, &.{"evil.com; rm -rf /"}, ""));
    try std.testing.expectError(error.InvalidDomain, build(a, &.{}, &.{"no-dot-here"}, ""));
    try std.testing.expectError(error.InvalidDomain, build(a, &.{}, &.{"a b.com"}, ""));
    try std.testing.expectError(error.InvalidDomain, build(a, &.{}, &.{"x.*"}, ""));
}

test "contains is exact, not substring or suffix" {
    const a = std.testing.allocator;
    var al = try build(a, &.{"github.com"}, &.{}, "");
    defer al.deinit();
    try std.testing.expect(al.contains("github.com"));
    try std.testing.expect(!al.contains("evil-github.com"));
    try std.testing.expect(!al.contains("github.co"));
}

test "deinit leaves an empty, re-deinit-safe value" {
    const a = std.testing.allocator;
    var al = try build(a, &.{"crates.io"}, &.{}, "");
    al.deinit();
    try std.testing.expectEqual(@as(usize, 0), al.names.len);
}

const std = @import("std");
