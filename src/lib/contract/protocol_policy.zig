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
/// later identity workstream.
pub const SandboxTier = enum { landlock_full, container_nested, macos_seatbelt, dev_none };

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
