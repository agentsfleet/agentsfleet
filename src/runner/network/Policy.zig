//! Policy.zig — the egress posture for a sandboxed lease: the switch between
//! egress *implementations*, selected by `RUNNER_NETWORK_POLICY`.
//!
//! A stateless namespace (the `std.mem` shape — no owned state). Three modes,
//! named so an operator reads the behaviour off the value (no "strict"/"secure"/
//! "mode" words that decay into mystery):
//!   allow_all          — everything outbound allowed: re-shares the host net
//!                        namespace (`--share-net`). The interim, UNENFORCED
//!                        posture while `allow_list_egress` is unbuilt. **Must be
//!                        opted into explicitly** (`RUNNER_NETWORK_POLICY=allow_all`)
//!                        — it is never the unset/typo fallback (that would
//!                        silently open egress, M100 §2 / Invariant 2).
//!   deny_all_egress    — no outbound traffic: net namespace unshared, NO veth.
//!   allow_list_egress  — outbound only to explicitly permitted destinations:
//!                        own netns + veth gated by the default-deny nftables
//!                        allowlist (`EgressScope`, option D). The allowlist is
//!                        the FULL per-lease set — operator registry baseline ∪
//!                        the agent's `network.allow` ∪ the inference host.
//!                        Opt-in; **fails closed (`egress_strict_unimplemented_fail_closed`)**
//!                        until that wiring lands — it never silently pretends to
//!                        enforce.
//!
//! `allow_all` and `allow_list_egress` are the abstraction's two implementations
//! of "the lease has network": flip the env var to move from unenforced
//! (interim) to kernel-enforced without code churn. `deny_all_egress` is the
//! no-network short-circuit.
//!
//! **Fail-closed default (M100 §2).** An unset or unrecognized
//! `RUNNER_NETWORK_POLICY` resolves to `allow_list_egress` — which fails CLOSED
//! at the supervisor (refuses the lease) until the `EgressScope` wiring lands —
//! NOT to `allow_all`. A misconfiguration therefore never silently grants open
//! egress; the operator must name `allow_all` explicitly to take the interim
//! open posture. This is the forward-compatible resolution: once `EgressScope`
//! lands, "unset" already means "kernel-enforced allowlist" rather than "open".

const std = @import("std");
const client_errors = @import("../engine/client_errors.zig");
const log = @import("log").scoped(.egress_policy);

const ALLOW_ALL = "allow_all";
const DENY_ALL_EGRESS = "deny_all_egress";
const ALLOW_LIST_EGRESS = "allow_list_egress";

/// The fail-closed posture an unset/unrecognized policy resolves to (M100 §2,
/// Invariant 2). Single-sourced (RULE UFS) — referenced by `fromSlice`'s
/// fallback and the parse tests so the two can never drift.
pub const FAIL_CLOSED_DEFAULT: Mode = .allow_list_egress;

pub const Mode = enum {
    /// Everything outbound allowed (re-shares host netns, `--share-net`).
    /// Opt-in only — never the unset/typo fallback (Invariant 2).
    allow_all,
    /// No outbound traffic: net namespace unshared, no veth.
    deny_all_egress,
    /// Outbound only to permitted destinations: own netns + veth + nftables
    /// allowlist. Opt-in / fail-closed default; fails closed until the
    /// `EgressScope` wiring lands.
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

    /// Operator-facing one-line posture, logged at startup (M100) so
    /// "is egress open?" is answerable from the boot log. Static strings — no
    /// allocation, safe to log directly.
    pub fn postureLabel(self: Mode) []const u8 {
        return switch (self) {
            .allow_all => "allow_all (OPEN egress — host netns shared; interim, UNENFORCED)",
            .deny_all_egress => "deny_all_egress (no outbound network)",
            .allow_list_egress => "allow_list_egress (strict allowlist — fails closed until EgressScope wiring lands)",
        };
    }
};

/// Parse `RUNNER_NETWORK_POLICY`. **Unset → `FAIL_CLOSED_DEFAULT`** (never
/// `allow_all`): a missing policy must not silently open egress (M100 §2,
/// Invariant 2). A set-but-unrecognized value is logged and also falls back to
/// the fail-closed default.
pub fn fromMap(env_map: *const std.process.Environ.Map) Mode {
    const raw = env_map.get("RUNNER_NETWORK_POLICY") orelse return FAIL_CLOSED_DEFAULT;
    return fromSlice(raw);
}

/// Parse a mode string (exact, case-insensitive). Exported for testing.
/// An unrecognized value is logged and resolves to `FAIL_CLOSED_DEFAULT` — a
/// typo fails closed (refuses the lease), never silently grants open egress.
pub fn fromSlice(raw: []const u8) Mode {
    if (std.ascii.eqlIgnoreCase(raw, ALLOW_ALL)) return .allow_all;
    if (std.ascii.eqlIgnoreCase(raw, DENY_ALL_EGRESS)) return .deny_all_egress;
    if (std.ascii.eqlIgnoreCase(raw, ALLOW_LIST_EGRESS)) return .allow_list_egress;
    log.warn("network_policy_unrecognized", .{ .error_code = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG, .value = raw, .fallback = @tagName(FAIL_CLOSED_DEFAULT) });
    return FAIL_CLOSED_DEFAULT;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "fromSlice parses all three modes, case-insensitively" {
    try std.testing.expectEqual(Mode.allow_all, fromSlice("allow_all"));
    try std.testing.expectEqual(Mode.allow_all, fromSlice("ALLOW_ALL"));
    try std.testing.expectEqual(Mode.deny_all_egress, fromSlice("deny_all_egress"));
    try std.testing.expectEqual(Mode.allow_list_egress, fromSlice("allow_list_egress"));
    try std.testing.expectEqual(Mode.allow_list_egress, fromSlice("Allow_List_Egress"));
}

test "fromSlice fails closed (allow_list_egress), never allow_all, on unknown / empty / typo" {
    // M100 §2 / Invariant 2: an unset/unrecognized policy must NOT open egress.
    // Every fallback case resolves to the fail-closed default and — critically —
    // is NOT allow_all (the assertion a value-flip mutation must trip).
    const fallback = [_][]const u8{
        "",                   "open_internet",
        "registry_allowlist", " allow_list_egress",
        "allow_list_egress ", "deny_all",
        "ALLOW_ALL ",         "allowall",
    };
    for (fallback) |raw| {
        try std.testing.expectEqual(FAIL_CLOSED_DEFAULT, fromSlice(raw));
        try std.testing.expect(fromSlice(raw) != .allow_all);
        try std.testing.expect(!fromSlice(raw).sharesHostNet());
    }
}

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
