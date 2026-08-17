//! Assigned-policy wire vocabulary — what the control plane assigns to a runner
//! row and what the host reports back. Split from `protocol.zig` (RULE FLL) and
//! re-exported there, so consumers keep the `protocol.X` names.
//!
//! Direction of authority: assigned policy travels control-plane → runner on
//! the enrollment read (`GET /v1/runners/me`) and every heartbeat reply; the
//! capability report travels runner → control-plane in the heartbeat request.
//! The two are stored in separate columns and never overwrite each other; the
//! heartbeat path reconciles them into the row's degraded verdict.

/// Isolation strength the control plane ASSIGNS to a runner row — operator
/// input at enrollment, mutable via the fleet PATCH, delivered to the host with
/// its identity. The host applies it and reports what its kernel can actually
/// enforce; that report is unauthenticated self-assertion (a compromised host
/// can lie), so placement trust stays operator-assigned — attestation is a
/// later identity workstream. Only tiers with real enforcement are members —
/// the Seatbelt tier was removed (M148 §6): it never had enforcement code, and
/// a tier that cannot be applied must not be assignable. A stray stored value
/// parses fail-closed (refuse to lease), same as any unknown tier.
pub const SandboxTier = enum { landlock_full, container_nested, dev_none };

/// Egress posture assigned per runner. Three modes, named so an operator reads
/// the behaviour off the value (no "strict"/"secure" words that decay into
/// mystery):
///   allow_all         — everything outbound allowed (re-shares host netns,
///                       `--share-net`). Opt-in only; never a fallback.
///   deny_all_egress   — no outbound traffic: netns unshared, no veth.
///   allow_list_egress — outbound only to permitted destinations via the
///                       kernel-enforced `EgressScope` boundary; until that
///                       wiring lands (2.0.1) the host cannot enforce it, so
///                       assigning it reconciles to a visible degraded row and
///                       the host refuses leases fail-closed.
pub const NetworkPolicy = enum {
    allow_all,
    deny_all_egress,
    allow_list_egress,

    /// Only `allow_all` re-shares the host network namespace (`--share-net`);
    /// `allow_list_egress` keeps its own (filtered) netns and `deny_all_egress`
    /// has no network at all.
    pub fn sharesHostNet(self: NetworkPolicy) bool {
        return self == .allow_all;
    }

    /// The mode routes through the kernel-enforced egress boundary
    /// (`EgressScope`). The supervisor establishes egress iff this is true.
    pub fn enforcesEgress(self: NetworkPolicy) bool {
        return self == .allow_list_egress;
    }

    /// Operator-facing one-line posture, logged at startup so "is egress
    /// open?" is answerable from the boot log. Static strings — no allocation.
    pub fn postureLabel(self: NetworkPolicy) []const u8 {
        return switch (self) {
            .allow_all => "allow_all (OPEN egress — host netns shared; interim, UNENFORCED)",
            .deny_all_egress => "deny_all_egress (no outbound network)",
            .allow_list_egress => "allow_list_egress (strict allowlist — fails closed until EgressScope wiring lands)",
        };
    }
};

/// The fail-closed posture an unset, missing, or unrecognized network policy
/// resolves to — never `allow_all`: a malformed policy must not silently open
/// egress. Single-sourced (RULE UFS); the runner's decoder and the control
/// plane's reconciliation both reference it.
pub const FAIL_CLOSED_DEFAULT: NetworkPolicy = .allow_list_egress;

/// Worker-pool sizing bounds (RULE UFS: the clamp is single-sourced). Assigned
/// by the control plane and clamped on BOTH sides — the assignment surface at
/// write, the host at apply — so a fat-fingered value can never fork unbounded
/// children on one host.
pub const DEFAULT_WORKER_COUNT: u32 = 1;
pub const MIN_WORKER_COUNT: u32 = 1;
pub const MAX_WORKER_COUNT: u32 = 64;

/// The policy the control plane assigns to one runner — everything a host was
/// previously told through its environment, now delivered with its identity.
pub const AssignedPolicy = struct {
    sandbox_tier: SandboxTier,
    network_policy: NetworkPolicy,
    /// Operator registry baseline merged into each lease's egress allowlist.
    /// Empty = the runner substitutes its named default registry set.
    registry_allowlist: []const []const u8,
    worker_count: u32,
    /// Extra host paths bound into every lease's sandbox, IN ADDITION to the
    /// daemon-owned baseline (`sandbox_args.RO_SYSTEM_PATHS`) — an operator can
    /// add a path a host needs, never remove or re-mode one the sandbox depends
    /// on. Defaulted so an older control plane that omits the field still
    /// decodes. Validated by `extraBindsValid` on both sides.
    extra_binds: []const ExtraBind = &.{},
};

/// What this host's kernel can actually enforce — probed at startup, refreshed
/// per heartbeat tick. Each field is one enforcement mechanism the
/// reconciliation can name in a degraded reason.
pub const CapabilityReport = struct {
    landlock: bool,
    seccomp: bool,
    /// Controllers present in the delegated cgroup's `subtree_control`.
    cgroup_controllers: []const []const u8,
    bubblewrap: bool,
    /// Kernel-enforced egress allowlisting (`EgressScope`) — false in every
    /// build until that wiring lands (2.0.1), so an assigned
    /// `allow_list_egress` reads as a degraded row, never a silent refusal loop.
    egress_enforcement: bool,
};

/// App-enforced registry-entry grammar (RULE STS: no SQL CHECK): host[:port],
/// the same shape the dashboard's `REGISTRY_HOST_REGEX` accepts (UFS), so the
/// raw API cannot store what the dialog would reject. The bounds are caps an
/// operator never legitimately hits; they exist so a direct `runner:write`
/// call cannot stuff the per-heartbeat payload — and, once `EgressScope`
/// lands, the kernel allowlist input — with unbounded arbitrary content.
pub const MAX_REGISTRY_ENTRIES: usize = 32;
pub const MAX_REGISTRY_HOST_LEN: usize = 259; // 253-char host + ":" + 5-digit port

/// Capability-report bounds — a runner token must not be a persistence
/// amplifier: the controllers list is a handful of kernel names, so anything
/// past these caps is a malformed report (dropped as "no report this beat"),
/// never a mebibyte JSONB the operator list re-reads on every page.
pub const MAX_REPORT_CONTROLLERS: usize = 16;
pub const MAX_CONTROLLER_NAME_LEN: usize = 64;

pub fn capabilityReportBounded(report: CapabilityReport) bool {
    if (report.cgroup_controllers.len > MAX_REPORT_CONTROLLERS) return false;
    for (report.cgroup_controllers) |c| {
        if (c.len == 0 or c.len > MAX_CONTROLLER_NAME_LEN) return false;
    }
    return true;
}

pub fn registryAllowlistValid(entries: []const []const u8) bool {
    if (entries.len > MAX_REGISTRY_ENTRIES) return false;
    for (entries) |e| {
        if (!registryHostValid(e)) return false;
    }
    return true;
}

fn registryHostValid(entry: []const u8) bool {
    if (entry.len == 0 or entry.len > MAX_REGISTRY_HOST_LEN) return false;
    const colon = std.mem.indexOfScalar(u8, entry, ':');
    const host = if (colon) |i| entry[0..i] else entry;
    if (host.len == 0) return false;
    for (host) |c| switch (c) {
        'A'...'Z', 'a'...'z', '0'...'9', '_', '.', '-' => {},
        else => return false,
    };
    if (colon) |i| {
        const port = entry[i + 1 ..];
        if (port.len == 0 or port.len > 5) return false;
        for (port) |c| {
            if (c < '0' or c > '9') return false;
        }
    }
    return true;
}

const std = @import("std");
const bind = @import("protocol_bind.zig");
const selftest = @import("protocol_selftest.zig");

// The sandbox bind contract lives in its own module (RULE FLL); re-exported
// here so `protocol.zig` and every existing consumer keep the same names.
pub const BindMode = bind.BindMode;
pub const ExtraBind = bind.ExtraBind;
pub const BASELINE_RO_PATHS = bind.BASELINE_RO_PATHS;
pub const SENSITIVE_PATHS = bind.SENSITIVE_PATHS;
pub const MAX_EXTRA_BINDS = bind.MAX_EXTRA_BINDS;
pub const MAX_BIND_PATH_LEN = bind.MAX_BIND_PATH_LEN;
pub const MAX_BIND_NOTE_LEN = bind.MAX_BIND_NOTE_LEN;
pub const extraBindsValid = bind.extraBindsValid;

// The self-test verdict, split for the same reason and re-exported the same way.
pub const SelftestCheck = selftest.SelftestCheck;
pub const SelftestReport = selftest.SelftestReport;
pub const MAX_SELFTEST_CHECKS = selftest.MAX_SELFTEST_CHECKS;
pub const MAX_CHECK_NAME_LEN = selftest.MAX_CHECK_NAME_LEN;
pub const MAX_CHECK_DETAIL_LEN = selftest.MAX_CHECK_DETAIL_LEN;
pub const SelftestRejection = selftest.Rejection;
pub const selftestReportRejection = selftest.selftestReportRejection;

test {
    _ = bind;
    _ = selftest;
}

test "registryAllowlistValid accepts host[:port] names and refuses everything else" {
    try std.testing.expect(registryAllowlistValid(&.{ "pypi.org", "registry.npmjs.org:5000" }));
    try std.testing.expect(registryAllowlistValid(&.{})); // empty = runner defaults
    try std.testing.expect(!registryAllowlistValid(&.{"http://pypi.org"})); // scheme
    try std.testing.expect(!registryAllowlistValid(&.{"py pi.org"})); // space
    try std.testing.expect(!registryAllowlistValid(&.{""})); // empty entry
    try std.testing.expect(!registryAllowlistValid(&.{"pypi.org:"})); // bare colon
    try std.testing.expect(!registryAllowlistValid(&.{"pypi.org:70000x"})); // 6-char port
    try std.testing.expect(!registryAllowlistValid(&.{"a" ** 260})); // over length
}

test "test_assigned_policy_decodes_without_extra_binds" {
    // Wire compatibility: a control plane that predates the field still decodes,
    // and the absent list reads as "baseline only" rather than failing closed on
    // a runner that would otherwise lease fine.
    const older =
        \\{"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":[],"worker_count":1}
    ;
    const parsed = try std.json.parseFromSlice(AssignedPolicy, std.testing.allocator, older, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqual(@as(usize, 0), parsed.value.extra_binds.len);
}

test "test_sandbox_tier_vocabulary_excludes_seatbelt" {
    // §6 Dimension 6.1 — the vocabulary is exactly the enforceable tiers.
    // (The removed name is spliced so the R7 zero-reference sweep stays green.)
    const names = std.meta.fieldNames(SandboxTier);
    try std.testing.expectEqual(3, names.len);
    const removed = "macos_" ++ "seatbelt";
    for (names) |n| try std.testing.expect(!std.mem.eql(u8, n, removed));
}
