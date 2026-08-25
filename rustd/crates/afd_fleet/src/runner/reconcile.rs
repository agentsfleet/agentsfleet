//! Assigned policy against reported capability, resolved to the row's verdict.
//!
//! Pure — no clock, no rows, no datastore — so the whole matrix is a unit test
//! and the heartbeat only orchestrates reads and writes around it. That
//! separation is `heartbeat_reconcile.zig`'s and it is the right one; what
//! changes here is the shape of the answer.
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
const REQUIRED_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

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

/// What an assigned tier needs the host to prove.
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
/// `dev_none` builds no cage, so it demands no mechanism. BOTH cage tiers
/// demand the full set — Landlock included for `container_nested`, because the
/// in-child hardening applies Landlock fail-closed on every sandboxed tier, so
/// a nested host without it would abort every lease. Demanding it here surfaces
/// that as a visible degraded row instead of a silent refusal loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TierNeeds {
    /// The tier builds no cage, so there is nothing for the host to prove.
    NoCage,
    /// The tier builds a cage, which needs every mechanism below it.
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

/// Whether `policy` needs kernel-enforced egress allowlisting.
const fn enforces_egress(policy: NetworkPolicy) -> bool {
    matches!(policy, NetworkPolicy::AllowListEgress)
}

/// Whether `policy` leaves the child on the host's own network namespace.
const fn shares_host_net(policy: NetworkPolicy) -> bool {
    matches!(policy, NetworkPolicy::AllowAll)
}

/// Resolves the assignment against what the host reported it can enforce.
///
/// Missing policy or missing report degrade on their own; otherwise the first
/// unmet mechanism, in a fixed order, names the reason. The order is stable so
/// an operator fixing one mechanism sees the next one rather than a reshuffled
/// answer.
#[must_use]
pub fn reconcile(
    assigned: Option<&AssignedPolicy<'_>>,
    achievable: Option<&CapabilityReport<'_>>,
) -> Verdict {
    let Some(policy) = assigned else {
        return Verdict::degraded(REASON_NO_ASSIGNED_POLICY);
    };
    let needs = TierNeeds::of(policy.sandbox_tier);
    let wants_egress = enforces_egress(policy.network_policy);

    // A posture other than `allow_all` is enforced BY the cage (network
    // namespace unshare, veth), so a cage-less tier structurally cannot deliver
    // it however the host is built. Degrade rather than render "no egress" over
    // a child that inherits the host namespace.
    if !shares_host_net(policy.network_policy) && !needs.any() {
        return Verdict::degraded(REASON_NETWORK_NEEDS_CAGE);
    }
    if !needs.any() && !wants_egress {
        return Verdict::Healthy;
    }

    let Some(cap) = achievable else {
        return Verdict::degraded(REASON_NO_CAPABILITY_REPORT);
    };
    // A table walked in order, rather than five `if` statements: the ORDER is
    // the documented part, and a table makes it the data it is instead of
    // something a reader reconstructs from statement sequence.
    let cage = needs.any();
    let checks = [
        (cage, cap.landlock, REASON_LANDLOCK_UNAVAILABLE),
        (cage, cap.seccomp, REASON_SECCOMP_UNAVAILABLE),
        (
            cage,
            has_required_controllers(&cap.cgroup_controllers),
            REASON_CGROUP_CONTROLLERS_MISSING,
        ),
        (cage, cap.bubblewrap, REASON_BUBBLEWRAP_MISSING),
        (
            wants_egress,
            cap.egress_enforcement,
            REASON_EGRESS_ENFORCEMENT_UNAVAILABLE,
        ),
    ];
    checks
        .into_iter()
        .find_map(|(needed, proven, reason)| {
            (needed && !proven).then_some(Verdict::degraded(reason))
        })
        .unwrap_or(Verdict::Healthy)
}

/// Whether every controller a cage needs is in the delegated subtree.
fn has_required_controllers(present: &[std::borrow::Cow<'_, str>]) -> bool {
    REQUIRED_CONTROLLERS
        .iter()
        .all(|required| present.iter().any(|found| found == required))
}
