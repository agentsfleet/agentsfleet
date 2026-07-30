//! Policy.zig — the egress posture for a sandboxed lease: the switch between
//! egress *implementations*, ASSIGNED per runner by the control plane (M148)
//! and delivered with the heartbeat — never read from the environment.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Three modes,
//! named so an operator reads the behaviour off the value (no "strict"/"secure"/
//! "mode" words that decay into mystery):
//!   allow_all          — everything outbound allowed: re-shares the host net
//!                        namespace (`--share-net`). The interim, UNENFORCED
//!                        posture while `allow_list_egress` is unbuilt. **Must be
//!                        assigned explicitly** — it is never the fail-closed
//!                        fallback (that would silently open egress, M100 §2 /
//!                        Invariant 2).
//!   deny_all_egress    — no outbound traffic: net namespace unshared, NO veth.
//!   allow_list_egress  — outbound only to explicitly permitted destinations:
//!                        own netns + veth gated by the default-deny nftables
//!                        allowlist (`EgressScope`, option D). The allowlist is
//!                        the FULL per-lease set — operator registry baseline ∪
//!                        the agent's `network.allow` ∪ the inference host.
//!                        **Fails closed (`egress_strict_unimplemented_fail_closed`)**
//!                        until that wiring lands — the capability report pins
//!                        `egress_enforcement=false`, so assigning this mode
//!                        reads as a degraded row, never a silent refusal loop.
//!
//! `allow_all` and `allow_list_egress` are the abstraction's two implementations
//! of "the lease has network": re-assign from the dashboard to move from
//! unenforced (interim) to kernel-enforced without code churn. `deny_all_egress`
//! is the no-network short-circuit.
//!
//! **Fail-closed default (M100 §2).** A missing or malformed assignment refuses
//! to lease outright (`AppliedPolicy` holds nothing), and `FAIL_CLOSED_DEFAULT`
//! names the posture every boot-time placeholder takes — `allow_list_egress`,
//! NOT `allow_all`. A misconfiguration therefore never silently grants open
//! egress; the operator must assign `allow_all` explicitly to take the interim
//! open posture.

const std = @import("std");
const contract = @import("contract");

// Mode tag names, used by the posture-label pin tests below.
const ALLOW_ALL = "allow_all";
const DENY_ALL_EGRESS = "deny_all_egress";
const ALLOW_LIST_EGRESS = "allow_list_egress";

/// The shared wire enum (`contract.protocol.NetworkPolicy`) — the control
/// plane authors this value and the runner applies it; this namespace keeps the
/// runner-side posture helpers. The methods (`sharesHostNet` /
/// `enforcesEgress` / `postureLabel`) travel with the enum in the contract.
pub const Mode = contract.protocol.NetworkPolicy;

/// The fail-closed posture the boot-time placeholder takes (M100 §2,
/// Invariant 2). Single-sourced in the contract (RULE UFS). The env-parse
/// layer that once fell back to this is gone (M148 removed the policy
/// environment surface); a missing or malformed ASSIGNMENT refuses to lease
/// outright (`AppliedPolicy` holds nothing) rather than resolving to any mode.
pub const FAIL_CLOSED_DEFAULT = contract.protocol.FAIL_CLOSED_DEFAULT;

// ── Tests ───────────────────────────────────────────────────────────────────

test "FAIL_CLOSED_DEFAULT is a fail-closed posture (never allow_all)" {
    try std.testing.expect(FAIL_CLOSED_DEFAULT != .allow_all);
    try std.testing.expect(!FAIL_CLOSED_DEFAULT.sharesHostNet());
    // It routes through the supervisor's fail-closed refusal until enforcement lands.
    try std.testing.expect(FAIL_CLOSED_DEFAULT.enforcesEgress());
}

test "strategy helpers: only allow_all shares host net; only allow_list_egress enforces" {
    try std.testing.expect(Mode.allow_all.sharesHostNet());
    try std.testing.expect(!Mode.allow_list_egress.sharesHostNet());
    try std.testing.expect(!Mode.deny_all_egress.sharesHostNet());

    try std.testing.expect(Mode.allow_list_egress.enforcesEgress());
    try std.testing.expect(!Mode.allow_all.enforcesEgress());
    try std.testing.expect(!Mode.deny_all_egress.enforcesEgress());
}

test "postureLabel names each posture (distinct, non-empty, operator-readable)" {
    // Distinct + non-empty (a label-swap or empty-string mutation trips this),
    // and each names its own tag so the boot log is unambiguous.
    const all = Mode.allow_all.postureLabel();
    const deny = Mode.deny_all_egress.postureLabel();
    const list = Mode.allow_list_egress.postureLabel();
    try std.testing.expect(all.len > 0 and deny.len > 0 and list.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, all, ALLOW_ALL) != null);
    try std.testing.expect(std.mem.indexOf(u8, deny, DENY_ALL_EGRESS) != null);
    try std.testing.expect(std.mem.indexOf(u8, list, ALLOW_LIST_EGRESS) != null);
    try std.testing.expect(!std.mem.eql(u8, all, deny));
    try std.testing.expect(!std.mem.eql(u8, deny, list));
    try std.testing.expect(!std.mem.eql(u8, all, list));
}

test "Mode has exactly three modes (no silent fourth)" {
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(Mode).@"enum".fields.len);
}
