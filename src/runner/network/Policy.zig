//! Policy.zig — the egress posture for a sandboxed lease: the switch between
//! two egress *implementations*, selected by `RUNNER_NETWORK_POLICY`.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Three postures:
//!   deny_all                 — net namespace unshared, NO veth: zero egress.
//!                              Default; dev + macOS.
//!   registry_allowlist       — INTERIM (until 2.0.1): re-shares the host net
//!                              namespace (`--share-net`) so the network-enabled
//!                              tier works (allow-all). The allowlist is advisory
//!                              (L7-only) in this posture — no kernel boundary.
//!   registry_allowlist_strict — STRICT: own netns + veth gated by the default-
//!                              deny nftables allowlist (`EgressScope`, option D).
//!                              The opt-in kernel-enforced posture — the egress
//!                              boundary this workstream is building. Until that
//!                              wiring lands (2.0.1) it FAILS CLOSED — selecting it
//!                              refuses the lease (`UZ-RUN-007`) rather than
//!                              silently pretending to enforce.
//!
//! The two `registry_allowlist*` postures are the abstraction's two
//! implementations: flip the env var to move between allow-all (interim) and
//! kernel-enforced (strict) without code churn. `deny_all` short-circuits both.

const std = @import("std");
const log = @import("log").scoped(.egress_policy);

const DENY_ALL = "deny_all";
const REGISTRY_ALLOWLIST = "registry_allowlist";
const REGISTRY_ALLOWLIST_STRICT = "registry_allowlist_strict";

pub const Mode = enum {
    /// No network: the net namespace is unshared and given no veth.
    deny_all,
    /// Interim allow-all: re-shares the host net namespace (`--share-net`).
    registry_allowlist,
    /// Strict, kernel-enforced egress (own netns + veth + nftables allowlist).
    /// Opt-in; fails closed until the `EgressScope` wiring lands.
    registry_allowlist_strict,

    /// The interim posture re-shares the host network namespace (`--share-net`).
    /// Only `registry_allowlist` does; strict keeps its own (filtered) netns and
    /// `deny_all` has no network at all.
    pub fn sharesHostNet(self: Mode) bool {
        return self == .registry_allowlist;
    }

    /// The strict posture routes through the kernel-enforced egress boundary
    /// (`EgressScope`). The supervisor establishes egress iff this is true.
    pub fn enforcesEgress(self: Mode) bool {
        return self == .registry_allowlist_strict;
    }
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
/// Order matters: the strict suffix is checked before the bare prefix.
pub fn fromSlice(raw: []const u8) Mode {
    if (std.ascii.eqlIgnoreCase(raw, REGISTRY_ALLOWLIST_STRICT)) return .registry_allowlist_strict;
    if (std.ascii.eqlIgnoreCase(raw, REGISTRY_ALLOWLIST)) return .registry_allowlist;
    if (std.ascii.eqlIgnoreCase(raw, DENY_ALL)) return .deny_all;
    log.warn("network_policy_unrecognized", .{ .value = raw, .fallback = DENY_ALL });
    return .deny_all;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "fromSlice parses all three postures, case-insensitively" {
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("registry_allowlist"));
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("REGISTRY_ALLOWLIST"));
    try std.testing.expectEqual(Mode.registry_allowlist_strict, fromSlice("registry_allowlist_strict"));
    try std.testing.expectEqual(Mode.registry_allowlist_strict, fromSlice("Registry_Allowlist_Strict"));
    try std.testing.expectEqual(Mode.deny_all, fromSlice("deny_all"));
}

test "fromSlice fails closed on unknown / empty / whitespace / injection" {
    const deny = [_][]const u8{
        "",                            "open_internet",
        " registry_allowlist",         "registry_allowlist ",
        "registry_allowlist\x00extra", "registry_allowlist; rm -rf /",
        "registry_allowlist\ndenied",  "registry_allowlist_stric",
    };
    for (deny) |raw| try std.testing.expectEqual(Mode.deny_all, fromSlice(raw));
}

test "strategy helpers: only interim shares host net; only strict enforces" {
    try std.testing.expect(Mode.registry_allowlist.sharesHostNet());
    try std.testing.expect(!Mode.registry_allowlist_strict.sharesHostNet());
    try std.testing.expect(!Mode.deny_all.sharesHostNet());

    try std.testing.expect(Mode.registry_allowlist_strict.enforcesEgress());
    try std.testing.expect(!Mode.registry_allowlist.enforcesEgress());
    try std.testing.expect(!Mode.deny_all.enforcesEgress());
}

test "Mode has exactly three postures (no silent fourth)" {
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(Mode).@"enum".fields.len);
}
