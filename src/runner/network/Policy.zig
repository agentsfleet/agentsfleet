//! Policy.zig — the egress posture for a sandboxed lease + its env parse.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Two postures:
//!   deny_all           — the child's net namespace is unshared with NO veth:
//!                        zero egress. Default; dev + macOS.
//!   registry_allowlist — own netns + a veth to the host, gated by the
//!                        default-deny nftables allowlist (`EgressScope`).
//!                        Bare-metal sets `RUNNER_NETWORK_POLICY=registry_allowlist`.
//!
//! Replaces the retired `engine/network.zig` posture parse. The `--share-net`
//! shared-host-netns model is GONE — `registry_allowlist` no longer re-shares
//! the host network; it gets a filtered veth instead.

const std = @import("std");
const log = @import("log").scoped(.egress_policy);

const DENY_ALL = "deny_all";
const REGISTRY_ALLOWLIST = "registry_allowlist";

pub const Mode = enum {
    /// No network: the child's net namespace is unshared and given no veth.
    deny_all,
    /// Own netns + veth, egress filtered to the merged allowlist by nftables.
    registry_allowlist,
};

/// Parse `RUNNER_NETWORK_POLICY`. Unset → `deny_all` (the secure default,
/// silent). A set-but-unrecognized value is logged — the misconfiguration
/// signal (a typo otherwise silently loses egress and every dependency install
/// fails until corrected). Fail-closed: anything non-exact resolves to deny_all.
pub fn fromMap(env_map: *const std.process.Environ.Map) Mode {
    const raw = env_map.get("RUNNER_NETWORK_POLICY") orelse return .deny_all;
    return fromSlice(raw);
}

/// Parse a posture string (exact, case-insensitive). Exported for testing.
pub fn fromSlice(raw: []const u8) Mode {
    if (std.ascii.eqlIgnoreCase(raw, REGISTRY_ALLOWLIST)) return .registry_allowlist;
    if (std.ascii.eqlIgnoreCase(raw, DENY_ALL)) return .deny_all;
    log.warn("network_policy_unrecognized", .{ .value = raw, .fallback = DENY_ALL });
    return .deny_all;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "fromSlice parses both postures, case-insensitively" {
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("registry_allowlist"));
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("REGISTRY_ALLOWLIST"));
    try std.testing.expectEqual(Mode.deny_all, fromSlice("deny_all"));
}

test "fromSlice fails closed on unknown / empty / whitespace / injection" {
    const deny = [_][]const u8{
        "",                            "open_internet",
        " registry_allowlist",         "registry_allowlist ",
        "registry_allowlist\x00extra", "registry_allowlist; rm -rf /",
        "registry_allowlist\ndenied",
    };
    for (deny) |raw| try std.testing.expectEqual(Mode.deny_all, fromSlice(raw));
}

test "Mode has exactly two variants (no silent third posture)" {
    try std.testing.expectEqual(@as(usize, 2), @typeInfo(Mode).@"enum".fields.len);
}
