//! Assigned policy against reported capability, resolved to the row's verdict.
//!
//! Pure — no clock, no rows, no datastore — so the whole matrix is a unit test
//! and the heartbeat only orchestrates reads and writes around it. That
//! separation is `heartbeat_reconcile.zig`'s and it is the right one; what
//! changes here is the shape of the answer, and what the question is asked in.
//!
//! # The daemon asks for OUTCOMES, not for mechanisms
//!
//! `heartbeat_reconcile.zig` asks a host whether it has Landlock, seccomp, the
//! `cpu`/`memory`/`pids` cgroup controllers and a `bubblewrap` binary. Every one
//! of those is a Linux implementation detail, and asking for them by name is
//! what makes the control plane bubblewrap-shaped: a runner backed by a
//! Firecracker microVM, a full virtual machine, or a managed platform isolate
//! delivers the same ISOLATION and would fail all four questions.
//!
//! So the vocabulary here is [`Guarantee`] — what a tenant is promised — and
//! [`Guarantee::proven_by`] is the only function in this crate that knows what
//! a Linux host reports. A substrate that delivers filesystem isolation by
//! giving each lease its own guest rootfs satisfies the same guarantee that
//! Landlock does, and nothing above the evidence table has to learn a second
//! vocabulary to say so.
//!
//! The wire is not there yet — `CapabilityReport` is still five Linux booleans,
//! frozen by M175 — so the evidence table is where the coupling now lives,
//! localised, in one place, with the mapping written down. When the report
//! grows a guarantee set of its own, that table is what gets deleted; nothing
//! else in this file changes.
//!
//! # A verdict is one value, not two fields that must agree
//!
//! The Zig `Verdict` is `{ degraded: bool, reason: ?[]const u8 }`, which can
//! spell two states that must never exist: degraded with no reason, and not
//! degraded WITH one. Nothing prevents either; the invariant lives in the
//! constructors and in a reader's memory. Here it is an enum, so
//! [`Verdict::Healthy`] has nowhere to put a reason and [`Verdict::Degraded`]
//! cannot omit one — `dispatch/write_rust.md` §Functional design, "two `Option`
//! fields where exactly one is ever set is an enum".
//!
//! The reason strings are the operator-facing vocabulary: they land on the
//! runner row verbatim and each names one missing mechanism, which maps to a
//! step in the runner bootstrap playbook. They are pinned byte-for-byte
//! against the Zig constants, because an operator greps for them.

use afd_wire::runner::{AssignedPolicy, CapabilityReport, NetworkPolicy, SandboxTier};

/// No assignment has been written to the row yet.
pub const REASON_NO_ASSIGNED_POLICY: &str = "no assigned policy";
/// The host has never reported what it can enforce.
pub const REASON_NO_CAPABILITY_REPORT: &str = "no capability report";
/// Filesystem isolation is unavailable on the host.
pub const REASON_LANDLOCK_UNAVAILABLE: &str = "landlock unavailable";
/// System-call filtering is unavailable on the host.
pub const REASON_SECCOMP_UNAVAILABLE: &str = "seccomp unavailable";
/// The delegated cgroup subtree is missing a controller the cage needs.
pub const REASON_CGROUP_CONTROLLERS_MISSING: &str = "cgroup controllers not delegated";
/// The sandbox launcher is absent.
pub const REASON_BUBBLEWRAP_MISSING: &str = "bubblewrap missing";
/// Kernel-enforced egress allowlisting is unavailable.
pub const REASON_EGRESS_ENFORCEMENT_UNAVAILABLE: &str = "egress enforcement unavailable";
/// An isolating network posture was assigned to a tier that builds no cage.
pub const REASON_NETWORK_NEEDS_CAGE: &str = "network isolation needs a sandbox tier";

/// Controllers a cage-building tier needs in the delegated subtree.
///
/// Mirrors the runner-side enablement set (`CgroupScope`: cpu, memory, pids).
/// Evidence for [`Guarantee::ResourceLimits`] on a Linux host, and nothing
/// beyond that — a substrate that caps a lease by giving it a vCPU and a memory
/// ceiling proves the same guarantee without a cgroup anywhere.
const REQUIRED_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

/// What a runner may take work under, stated as an outcome.
///
/// The unit an assignment is expressed in and a verdict refuses in. Each names
/// something a tenant's code cannot do, or cannot exceed, while a lease runs —
/// never how a host arranges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guarantee {
    /// A lease sees only the filesystem it was given.
    FilesystemIsolation,
    /// A lease cannot make system calls outside the permitted set.
    SyscallFiltering,
    /// A lease cannot exceed its processor, memory or process budget.
    ResourceLimits,
    /// A lease's processes cannot escape into the host's.
    ProcessContainment,
    /// A lease reaches only the destinations its policy permits.
    EgressControl,
}

impl Guarantee {
    /// What an operator is told when this guarantee is not proven.
    ///
    /// Still the Zig sentence, byte-for-byte, and deliberately so: an operator
    /// greps for these and a runbook names them. They read as mechanisms
    /// because today's only substrate is a Linux host — the day a second one
    /// reports, these become the guarantee's own words and the mapping is one
    /// edit in this function.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::FilesystemIsolation => REASON_LANDLOCK_UNAVAILABLE,
            Self::SyscallFiltering => REASON_SECCOMP_UNAVAILABLE,
            Self::ResourceLimits => REASON_CGROUP_CONTROLLERS_MISSING,
            Self::ProcessContainment => REASON_BUBBLEWRAP_MISSING,
            Self::EgressControl => REASON_EGRESS_ENFORCEMENT_UNAVAILABLE,
        }
    }

    /// Whether `report` is evidence that this host delivers this guarantee.
    ///
    /// **The one substrate-aware function in this crate.** Every field it reads
    /// is a Linux mechanism, because the wire report is a Linux report; a
    /// Firecracker or managed-platform runner would prove the same guarantees
    /// through different fields, and this is where that mapping would go until
    /// the report carries guarantees directly.
    #[must_use]
    pub fn proven_by(self, report: &CapabilityReport<'_>) -> bool {
        match self {
            Self::FilesystemIsolation => report.landlock,
            Self::SyscallFiltering => report.seccomp,
            Self::ResourceLimits => has_required_controllers(&report.cgroup_controllers),
            Self::ProcessContainment => report.bubblewrap,
            Self::EgressControl => report.egress_enforcement,
        }
    }
}

/// The guarantees a cage-building isolation class demands, in refusal order.
///
/// The ORDER is the documented part — an operator fixing one mechanism sees the
/// next one rather than a reshuffled answer — so it is a table rather than a
/// sequence of `if` statements that a reader has to reconstruct.
const CAGE_GUARANTEES: [Guarantee; 4] = [
    Guarantee::FilesystemIsolation,
    Guarantee::SyscallFiltering,
    Guarantee::ResourceLimits,
    Guarantee::ProcessContainment,
];

/// Whether a runner may take work, and why not when it may not.
///
/// An enum rather than a `bool` beside an `Option<&str>`, so the two illegal
/// combinations the Zig struct can spell are unrepresentable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The host proved everything its assignment demands.
    Healthy,
    /// Something the assignment demands is missing, and this is what.
    Degraded {
        /// One of this module's `REASON_*` constants. Always `'static`: a
        /// verdict is never row-owned text, so it can be written to a row and
        /// echoed on a heartbeat without a lifetime following it.
        reason: &'static str,
    },
}

impl Verdict {
    /// Whether this verdict bars the runner from taking work.
    #[must_use]
    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::Degraded { .. })
    }

    /// The reason, when there is one.
    ///
    /// The column is nullable and the wire field is optional, so both take this
    /// directly rather than each deciding what a healthy verdict writes.
    #[must_use]
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Healthy => None,
            Self::Degraded { reason } => Some(reason),
        }
    }

    /// A degraded verdict naming `reason`.
    const fn degraded(reason: &'static str) -> Self {
        Self::Degraded { reason }
    }
}

/// Whether an assigned tier builds a cage at all.
///
/// # Why this is an enum and not four booleans
///
/// The Zig `TierNeeds` is a struct of four `bool`s, which spells sixteen
/// states. Exactly TWO are reachable — `tierNeeds` returns all-false or
/// all-true and nothing else — so fourteen of them exist only to be impossible.
/// Clippy notices the shape (`struct_excessive_bools`); the fix is not to
/// silence it but to say what is actually true, which is that a tier either
/// builds a cage or does not (`dispatch/write_rust.md` §Functional design,
/// "make illegal states unrepresentable").
///
/// `dev_none` builds no cage, so it demands no guarantee. BOTH cage tiers
/// demand the full set — filesystem isolation included for `container_nested`,
/// because the in-child hardening applies Landlock fail-closed on every
/// sandboxed tier, so a nested host without it would abort every lease.
/// Demanding it here surfaces that as a visible degraded row instead of a
/// silent refusal loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TierNeeds {
    /// The tier builds no cage, so there is nothing for the host to prove.
    NoCage,
    /// The tier builds a cage, which needs every guarantee below it.
    Cage,
}

impl TierNeeds {
    /// Whether this tier demands anything at all.
    const fn any(self) -> bool {
        matches!(self, Self::Cage)
    }

    /// What `tier` demands.
    const fn of(tier: SandboxTier) -> Self {
        match tier {
            SandboxTier::LandlockFull | SandboxTier::ContainerNested => Self::Cage,
            SandboxTier::DevNone => Self::NoCage,
        }
    }
}

/// Whether `policy` needs enforced egress control.
const fn enforces_egress(policy: NetworkPolicy) -> bool {
    matches!(policy, NetworkPolicy::AllowListEgress)
}

/// Whether `policy` leaves the child on the host's own network namespace.
const fn shares_host_net(policy: NetworkPolicy) -> bool {
    matches!(policy, NetworkPolicy::AllowAll)
}

/// Every guarantee `policy` demands, in the order they are refused in.
///
/// The assignment's whole meaning, as a list: a cage tier demands the four that
/// build one, and an allowlisted egress posture demands control over where a
/// lease can reach, whichever tier it runs under.
fn demanded(policy: &AssignedPolicy<'_>) -> impl Iterator<Item = Guarantee> {
    let cage = TierNeeds::of(policy.sandbox_tier).any();
    CAGE_GUARANTEES
        .into_iter()
        .filter(move |_cage_guarantee| cage)
        .chain(enforces_egress(policy.network_policy).then_some(Guarantee::EgressControl))
}

/// Resolves the assignment against what the host reported it can deliver.
///
/// Missing policy or missing report degrade on their own; otherwise the first
/// unmet guarantee, in a fixed order, names the reason.
#[must_use]
pub fn reconcile(
    assigned: Option<&AssignedPolicy<'_>>,
    achievable: Option<&CapabilityReport<'_>>,
) -> Verdict {
    let Some(policy) = assigned else {
        return Verdict::degraded(REASON_NO_ASSIGNED_POLICY);
    };

    // A posture other than `allow_all` is enforced BY the cage (network
    // namespace unshare, veth), so a cage-less tier structurally cannot deliver
    // it however the host is built. Degrade rather than render "no egress" over
    // a child that inherits the host namespace.
    if !shares_host_net(policy.network_policy) && !TierNeeds::of(policy.sandbox_tier).any() {
        return Verdict::degraded(REASON_NETWORK_NEEDS_CAGE);
    }

    let mut demanded = demanded(policy).peekable();
    if demanded.peek().is_none() {
        // An assignment that demands nothing is satisfied by any host, and by a
        // host that has never reported. Checked before the report so a
        // `dev_none` runner is not held degraded waiting for a probe whose
        // answer could not change the verdict.
        return Verdict::Healthy;
    }

    let Some(report) = achievable else {
        return Verdict::degraded(REASON_NO_CAPABILITY_REPORT);
    };
    demanded
        .find(|guarantee| !guarantee.proven_by(report))
        .map_or(Verdict::Healthy, |unmet| Verdict::degraded(unmet.reason()))
}

/// Whether every controller a cage needs is in the delegated subtree.
fn has_required_controllers(present: &[std::borrow::Cow<'_, str>]) -> bool {
    REQUIRED_CONTROLLERS
        .iter()
        .all(|required| present.iter().any(|found| found == required))
}
