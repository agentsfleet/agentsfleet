//! Policy.zig — the egress posture for a sandboxed lease: the switch between
//! egress *implementations*, selected by `RUNNER_NETWORK_POLICY`.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Three modes,
//! named so an operator reads the behaviour off the value (no "strict"/"secure"/
//! "mode" words that decay into mystery):
//!   allow_all          — DEFAULT (unset → here). Everything outbound allowed:
//!                        re-shares the host net namespace (`--share-net`). The
//!                        interim, unenforced posture while `allow_list_egress`
//!                        is unbuilt.
//!   deny_all_egress    — no outbound traffic: net namespace unshared, NO veth.
//!   allow_list_egress  — outbound only to explicitly permitted destinations:
//!                        own netns + veth gated by the default-deny nftables
//!                        allowlist (`EgressScope`, option D). The allowlist is
//!                        the FULL per-lease set — operator registry baseline ∪
//!                        the zombie's `network.allow` ∪ the inference host.
//!                        Opt-in; **fails closed (`UZ-RUN-007`)** until that
//!                        wiring lands — it never silently pretends to enforce.
//!
//! `allow_all` and `allow_list_egress` are the abstraction's two implementations
//! of "the lease has network": flip the env var to move from unenforced
//! (interim) to kernel-enforced without code churn. `deny_all_egress` is the
//! no-network short-circuit. **Default is `allow_all`** (open) for the
//! pre-enforcement interim; it should flip to `allow_list_egress` once the
//! `EgressScope` wiring lands.

const std = @import("std");
const log = @import("log").scoped(.egress_policy);

const ALLOW_ALL = "allow_all";
const DENY_ALL_EGRESS = "deny_all_egress";
const ALLOW_LIST_EGRESS = "allow_list_egress";

pub const Mode = enum {
    /// Default: everything outbound allowed (re-shares host netns, `--share-net`).
    allow_all,
    /// No outbound traffic: net namespace unshared, no veth.
    deny_all_egress,
    /// Outbound only to permitted destinations: own netns + veth + nftables
    /// allowlist. Opt-in; fails closed until the `EgressScope` wiring lands.
    allow_list_egress,

    /// The mode re-shares the host network namespace (`--share-net`). Only
    /// `allow_all` does; `allow_list_egress` keeps its own (filtered) netns and
    /// `deny_all_egress` has no network at all.
    pub fn sharesHostNet(self: Mode) bool {
        return self == .allow_all;
    }

    /// The mode routes through the kernel-enforced egress boundary
    /// (`EgressScope`). The supervisor establishes egress iff this is true.
    pub fn enforcesEgress(self: Mode) bool {
        return self == .allow_list_egress;
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

/// Parse a mode string (exact, case-insensitive). Exported for testing.
pub fn fromSlice(raw: []const u8) Mode {
    if (std.ascii.eqlIgnoreCase(raw, ALLOW_ALL)) return .allow_all;
    if (std.ascii.eqlIgnoreCase(raw, DENY_ALL_EGRESS)) return .deny_all_egress;
    if (std.ascii.eqlIgnoreCase(raw, ALLOW_LIST_EGRESS)) return .allow_list_egress;
    log.warn("network_policy_unrecognized", .{ .value = raw, .fallback = ALLOW_ALL });
    return .allow_all;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "fromSlice parses all three modes, case-insensitively" {
    try std.testing.expectEqual(Mode.allow_all, fromSlice("allow_all"));
    try std.testing.expectEqual(Mode.allow_all, fromSlice("ALLOW_ALL"));
    try std.testing.expectEqual(Mode.deny_all_egress, fromSlice("deny_all_egress"));
    try std.testing.expectEqual(Mode.allow_list_egress, fromSlice("allow_list_egress"));
    try std.testing.expectEqual(Mode.allow_list_egress, fromSlice("Allow_List_Egress"));
}

test "fromSlice falls back to the allow_all default on unknown / empty / typo" {
    // The retired `registry_allowlist` value is now unrecognized → allow_all
    // (so a stale prod env keeps egress rather than failing closed).
    const fallback = [_][]const u8{
        "",                   "open_internet",
        "registry_allowlist", " allow_list_egress",
        "allow_list_egress ", "deny_all",
    };
    for (fallback) |raw| try std.testing.expectEqual(Mode.allow_all, fromSlice(raw));
}

test "strategy helpers: only allow_all shares host net; only allow_list_egress enforces" {
    try std.testing.expect(Mode.allow_all.sharesHostNet());
    try std.testing.expect(!Mode.allow_list_egress.sharesHostNet());
    try std.testing.expect(!Mode.deny_all_egress.sharesHostNet());

    try std.testing.expect(Mode.allow_list_egress.enforcesEgress());
    try std.testing.expect(!Mode.allow_all.enforcesEgress());
    try std.testing.expect(!Mode.deny_all_egress.enforcesEgress());
}

test "Mode has exactly three modes (no silent fourth)" {
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(Mode).@"enum".fields.len);
}
