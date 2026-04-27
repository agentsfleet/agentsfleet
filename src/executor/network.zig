//! Network policy enforcement for the host backend (M16_003 §3).
//!
//! Two policies:
//!   deny_all          — isolate the network namespace (--unshare-net via bwrap).
//!                       Default; dev and macOS use this.
//!   registry_allowlist — share the host network namespace (bwrap --share-net)
//!                       so package managers can reach public registries.
//!                       Bare-metal deploy sets EXECUTOR_NETWORK_POLICY=registry_allowlist.
//!
//! Allowlist hostnames are compile-time constants in executor_network_policy.zig.
//! Full TCP-layer enforcement (nftables) is Phase 2 — out of scope here.

const std = @import("std");
const builtin = @import("builtin");

const policy_config = @import("executor_network_policy.zig");

const log = std.log.scoped(.executor_network);

pub const NetworkPolicy = enum {
    /// No network access (default). Uses --unshare-net via bubblewrap.
    deny_all,
    /// Allow egress to the public package registries defined in
    /// executor_network_policy.REGISTRY_ALLOWLIST. Uses --share-net.
    registry_allowlist,
};

pub const NetworkConfig = struct {
    policy: NetworkPolicy = .deny_all,
};

/// Parse EXECUTOR_NETWORK_POLICY env var. Returns .deny_all if unset or unknown.
pub fn policyFromEnv(alloc: std.mem.Allocator) NetworkPolicy {
    const raw = std.process.getEnvVarOwned(alloc, "EXECUTOR_NETWORK_POLICY") catch return .deny_all;
    defer alloc.free(raw);
    return policyFromSlice(raw);
}

/// Parse a network policy string. Exported for unit testing.
fn policyFromSlice(raw: []const u8) NetworkPolicy {
    if (std.ascii.eqlIgnoreCase(raw, "registry_allowlist")) return .registry_allowlist;
    return .deny_all;
}

/// Append bubblewrap network arguments based on policy.
///
/// deny_all:          no-op — --unshare-all (used elsewhere) already includes
///                    --unshare-net.
/// registry_allowlist: inject --share-net so bwrap does NOT unshare the network
///                    namespace, then debug-log each permitted hostname.
pub fn appendBwrapNetworkArgs(
    alloc: std.mem.Allocator,
    argv: *std.ArrayList([]const u8),
    config: NetworkConfig,
    execution_id: []const u8,
) !void {
    switch (config.policy) {
        .deny_all => {
            // --unshare-all already enforces network isolation. No extra args.
        },
        .registry_allowlist => {
            // --share-net keeps the host network namespace so package managers
            // can reach public registries. Phase 2 will add nftables rules
            // to restrict egress to REGISTRY_ALLOWLIST only.
            try argv.append(alloc, "--share-net");
            for (policy_config.REGISTRY_ALLOWLIST) |host| {
                log.debug(
                    "network.allowlist host={s} execution_id={s}",
                    .{ host, execution_id },
                );
            }
        },
    }
}

/// Check if network namespace isolation is available on this host.
fn isNetworkNamespaceAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    std.fs.accessAbsolute("/usr/bin/bwrap", .{}) catch {
        std.fs.accessAbsolute("/usr/local/bin/bwrap", .{}) catch return false;
    };
    return true;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "deny_all is default policy" {
    const config = NetworkConfig{};
    try std.testing.expectEqual(NetworkPolicy.deny_all, config.policy);
}

test "appendBwrapNetworkArgs with deny_all adds no args" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .deny_all }, "test-exec-id");
    try std.testing.expectEqual(@as(usize, 0), argv.items.len);
}

test "appendBwrapNetworkArgs with registry_allowlist adds --share-net" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .registry_allowlist }, "test-exec-id");
    try std.testing.expectEqual(@as(usize, 1), argv.items.len);
    try std.testing.expectEqualStrings("--share-net", argv.items[0]);
}

test "policyFromSlice parses registry_allowlist" {
    try std.testing.expectEqual(NetworkPolicy.registry_allowlist, policyFromSlice("registry_allowlist"));
}

test "policyFromSlice is case-insensitive" {
    try std.testing.expectEqual(NetworkPolicy.registry_allowlist, policyFromSlice("REGISTRY_ALLOWLIST"));
    try std.testing.expectEqual(NetworkPolicy.registry_allowlist, policyFromSlice("Registry_Allowlist"));
}

test "policyFromSlice falls back to deny_all for unknown value" {
    try std.testing.expectEqual(NetworkPolicy.deny_all, policyFromSlice("open_internet"));
    try std.testing.expectEqual(NetworkPolicy.deny_all, policyFromSlice(""));
}

test "REGISTRY_ALLOWLIST contains expected hosts" {
    const hosts = policy_config.REGISTRY_ALLOWLIST;
    var found_npm = false;
    var found_pypi = false;
    var found_crates = false;
    var found_golang = false;
    for (hosts) |h| {
        if (std.mem.eql(u8, h, "registry.npmjs.org")) found_npm = true;
        if (std.mem.eql(u8, h, "pypi.org")) found_pypi = true;
        if (std.mem.eql(u8, h, "crates.io")) found_crates = true;
        if (std.mem.eql(u8, h, "proxy.golang.org")) found_golang = true;
    }
    try std.testing.expect(found_npm);
    try std.testing.expect(found_pypi);
    try std.testing.expect(found_crates);
    try std.testing.expect(found_golang);
}

test "isNetworkNamespaceAvailable returns false on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expect(!isNetworkNamespaceAvailable());
}

// ── T9 — Parameterized policyFromSlice ────────────────────────────────────────

test "policyFromSlice parameterized — all variants and edge inputs" {
    const Case = struct { input: []const u8, want: NetworkPolicy };
    const cases = [_]Case{
        // Happy path — exact and case variants
        .{ .input = "registry_allowlist", .want = .registry_allowlist },
        .{ .input = "REGISTRY_ALLOWLIST", .want = .registry_allowlist },
        .{ .input = "Registry_Allowlist", .want = .registry_allowlist },
        .{ .input = "REGISTRY_Allowlist", .want = .registry_allowlist },
        // Deny-all — unknown / empty / partial
        .{ .input = "", .want = .deny_all },
        .{ .input = "deny_all", .want = .deny_all },
        .{ .input = "open_internet", .want = .deny_all },
        .{ .input = "allowlist", .want = .deny_all },
        // Leading/trailing whitespace is NOT stripped — stays deny_all
        .{ .input = " registry_allowlist", .want = .deny_all },
        .{ .input = "registry_allowlist ", .want = .deny_all },
    };
    for (cases) |c| {
        const got = policyFromSlice(c.input);
        try std.testing.expectEqual(c.want, got);
    }
}

// ── T2 — Edge cases ───────────────────────────────────────────────────────────

test "appendBwrapNetworkArgs with deny_all ignores execution_id content" {
    // Empty execution_id should still produce no args under deny_all.
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .deny_all }, "");
    try std.testing.expectEqual(@as(usize, 0), argv.items.len);
}

test "appendBwrapNetworkArgs with registry_allowlist accepts empty execution_id" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .registry_allowlist }, "");
    try std.testing.expectEqual(@as(usize, 1), argv.items.len);
    try std.testing.expectEqualStrings("--share-net", argv.items[0]);
}

// ── T10 — Enum shape / constants ──────────────────────────────────────────────

test "NetworkPolicy enum has exactly two variants" {
    // Guards against accidental addition of a third policy that bypasses the
    // deny_all default enforcement.
    const field_count = @typeInfo(NetworkPolicy).@"enum".fields.len;
    try std.testing.expectEqual(@as(usize, 2), field_count);
}

test "NetworkConfig default policy is deny_all" {
    // Regression: the default must never silently become registry_allowlist.
    const cfg: NetworkConfig = .{};
    try std.testing.expectEqual(NetworkPolicy.deny_all, cfg.policy);
    try std.testing.expect(cfg.policy != .registry_allowlist);
}

// ── T11 — Memory safety ───────────────────────────────────────────────────────

test "appendBwrapNetworkArgs with registry_allowlist does not leak across multiple calls" {
    // std.testing.allocator panics on leak — this covers T11 automatically.
    const alloc = std.testing.allocator;
    for (0..10) |i| {
        var argv = std.ArrayList([]const u8){};
        defer argv.deinit(alloc);
        const exec_id = try std.fmt.allocPrint(alloc, "exec-id-{d}", .{i});
        defer alloc.free(exec_id);
        try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .registry_allowlist }, exec_id);
        try std.testing.expectEqual(@as(usize, 1), argv.items.len);
        try std.testing.expectEqualStrings("--share-net", argv.items[0]);
    }
}

// ── T8 — OWASP Agent Security: network policy fail-closed ────────────────────

// T8.1 — policyFromSlice with injection-like values falls back to deny_all.
// Guards against env var injection that attempts to widen the network policy.
// All variants below must resolve to deny_all (fail-closed).
test "T8: policyFromSlice with injection-like values fails closed to deny_all (M16_003 §3)" {
    const cases = [_][]const u8{
        "registry_allowlist\x00extra", // null byte suffix after valid prefix
        "registry_allowlist\ndenied", // newline injection
        "registry_allowlist; rm -rf /", // shell metachar injection
        "registry_allowlist' OR '1'='1", // SQL injection style
        " registry_allowlist", // leading whitespace (not stripped)
        "REGISTRY_ALLOWLIST\x00", // upper-case valid prefix + null byte
    };
    for (cases) |input| {
        const got = policyFromSlice(input);
        try std.testing.expectEqual(NetworkPolicy.deny_all, got);
    }
}

// T8.2 — appendBwrapNetworkArgs with deny_all adds zero args regardless of execution_id content.
// Guards against an execution_id that contains --share-net being interpreted as a flag.
test "T8: appendBwrapNetworkArgs deny_all ignores execution_id containing flag-like content" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    // execution_id containing what looks like a bwrap flag — must not add args.
    try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .deny_all }, "--share-net");
    try std.testing.expectEqual(@as(usize, 0), argv.items.len);
}

// T8.3 — registry_allowlist adds exactly one arg (--share-net), never more.
// Guards against a future regression where execution_id content triggers extra args.
test "T8: appendBwrapNetworkArgs registry_allowlist adds exactly one arg regardless of execution_id" {
    const alloc = std.testing.allocator;
    const exec_ids = [_][]const u8{
        "--share-net",
        "--unshare-net --share-net",
        "exec-id; --share-net",
        "normal-exec-id",
    };
    for (exec_ids) |exec_id| {
        var argv = std.ArrayList([]const u8){};
        defer argv.deinit(alloc);
        try appendBwrapNetworkArgs(alloc, &argv, .{ .policy = .registry_allowlist }, exec_id);
        // Exactly one arg must be added — no duplication from exec_id content.
        try std.testing.expectEqual(@as(usize, 1), argv.items.len);
        try std.testing.expectEqualStrings("--share-net", argv.items[0]);
    }
}
