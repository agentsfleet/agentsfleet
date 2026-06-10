//! Policy.zig — the egress posture for a sandboxed lease: the switch between
//! egress *implementations*, selected by `RUNNER_NETWORK_POLICY`.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Three postures:
//!   allow_all          — DEFAULT (unset → here). Re-shares the host net
//!                        namespace (`--share-net`): the lease reaches anything.
//!                        Honest name for the interim, unenforced egress while
//!                        the kernel allowlist (`registry_allowlist`) is unbuilt.
//!   deny_all           — net namespace unshared, NO veth: zero egress.
//!   registry_allowlist — STRICT, kernel-enforced: own netns + veth gated by the
//!                        default-deny nftables allowlist (`EgressScope`,
//!                        option D). Opt-in; **fails closed (`UZ-RUN-007`)**
//!                        until that wiring lands — it never silently pretends
//!                        to enforce. (This is the posture whose name is *true*:
//!                        an actual allowlist, not "allow everything".)
//!
//! `allow_all` and `registry_allowlist` are the abstraction's two
//! implementations of "the lease has network": flip the env var to move from
//! unenforced (interim) to kernel-enforced without code churn. `deny_all` is
//! the no-network short-circuit. **Default is `allow_all`** (open) for the
//! pre-enforcement interim; it should flip to the enforced posture once the
//! `EgressScope` wiring lands.

const std = @import("std");
const log = @import("log").scoped(.egress_policy);

const ALLOW_ALL = "allow_all";
const DENY_ALL = "deny_all";
const REGISTRY_ALLOWLIST = "registry_allowlist";

pub const Mode = enum {
    /// Default: re-shares the host net namespace (`--share-net`) — full egress.
    allow_all,
    /// No network: the net namespace is unshared and given no veth.
    deny_all,
    /// Strict, kernel-enforced egress (own netns + veth + nftables allowlist).
    /// Opt-in; fails closed until the `EgressScope` wiring lands.
    registry_allowlist,

    /// The posture re-shares the host network namespace (`--share-net`). Only
    /// `allow_all` does; `registry_allowlist` keeps its own (filtered) netns and
    /// `deny_all` has no network at all.
    pub fn sharesHostNet(self: Mode) bool {
        return self == .allow_all;
    }

    /// The posture routes through the kernel-enforced egress boundary
    /// (`EgressScope`). The supervisor establishes egress iff this is true.
    pub fn enforcesEgress(self: Mode) bool {
        return self == .registry_allowlist;
    }
};

/// Parse `RUNNER_NETWORK_POLICY`. **Unset → `allow_all`** (the open default for
/// the pre-enforcement interim). A set-but-unrecognized value is logged and
/// also falls back to the `allow_all` default — consistent baseline rather than
/// a surprise no-network on a typo.
pub fn fromMap(env_map: *const std.process.Environ.Map) Mode {
    const raw = env_map.get("RUNNER_NETWORK_POLICY") orelse return .allow_all;
    return fromSlice(raw);
}

/// Parse a posture string (exact, case-insensitive). Exported for testing.
pub fn fromSlice(raw: []const u8) Mode {
    if (std.ascii.eqlIgnoreCase(raw, ALLOW_ALL)) return .allow_all;
    if (std.ascii.eqlIgnoreCase(raw, DENY_ALL)) return .deny_all;
    if (std.ascii.eqlIgnoreCase(raw, REGISTRY_ALLOWLIST)) return .registry_allowlist;
    log.warn("network_policy_unrecognized", .{ .value = raw, .fallback = ALLOW_ALL });
    return .allow_all;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "fromSlice parses all three postures, case-insensitively" {
    try std.testing.expectEqual(Mode.allow_all, fromSlice("allow_all"));
    try std.testing.expectEqual(Mode.allow_all, fromSlice("ALLOW_ALL"));
    try std.testing.expectEqual(Mode.deny_all, fromSlice("deny_all"));
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("registry_allowlist"));
    try std.testing.expectEqual(Mode.registry_allowlist, fromSlice("Registry_Allowlist"));
}

test "fromSlice falls back to the allow_all default on unknown / empty / typo" {
    const fallback = [_][]const u8{
        "",                    "open_internet",
        " registry_allowlist", "registry_allowlist ",
        "registry_alowlist",   "deny",
    };
    for (fallback) |raw| try std.testing.expectEqual(Mode.allow_all, fromSlice(raw));
}

test "strategy helpers: only allow_all shares host net; only registry_allowlist enforces" {
    try std.testing.expect(Mode.allow_all.sharesHostNet());
    try std.testing.expect(!Mode.registry_allowlist.sharesHostNet());
    try std.testing.expect(!Mode.deny_all.sharesHostNet());

    try std.testing.expect(Mode.registry_allowlist.enforcesEgress());
    try std.testing.expect(!Mode.allow_all.enforcesEgress());
    try std.testing.expect(!Mode.deny_all.enforcesEgress());
}

test "Mode has exactly three postures (no silent fourth)" {
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(Mode).@"enum".fields.len);
}
