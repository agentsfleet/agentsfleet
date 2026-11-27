//! Assigned policy against reported capability, resolved to the row's verdict.
//!
//! Pure — no clock, no rows, no datastore — so the whole matrix is a unit test
//! and the heartbeat only orchestrates reads and writes around it.
//!
//! # A tier names its engine
//!
//! Each tier that builds a sandbox names the engine that builds it, and the
//! runner's report names the engine it runs. The verdict compares the two: a
//! `firecracker` runner proves its tier by reporting `engine: firecracker`
//! behind a hardware virtual machine, whose own kernel confines files, system
//! calls and processes, whose size caps its resources, and whose network the
//! host holds to the assigned posture. The daemon asks a host for no Linux
//! mechanism by name.
//!
//! # A verdict is one value, not two fields that must agree
//!
//! A `bool` beside an optional reason can spell two states that must never
//! exist: degraded with no reason, and not degraded WITH one. Here it is an
//! enum, so [`Verdict::Healthy`] has nowhere to put a reason and
//! [`Verdict::Degraded`] cannot omit one.
//!
//! The reason strings are the operator-facing vocabulary: they land on the
//! runner row verbatim, and an operator greps for them. Change their bytes
//! only on purpose.

use af_wire::runner::{
    AssignedPolicy, CapabilityReport, Isolation, NetworkPolicy, SandboxEngine, SandboxTier,
};

/// The host has never reported what it can enforce.
pub const REASON_NO_CAPABILITY_REPORT: &str = "no capability report";
/// An isolating network posture was assigned to a tier that builds no sandbox.
pub const REASON_NETWORK_NEEDS_CAGE: &str = "network isolation needs a sandbox tier";
/// The runner reports an engine, or a wall, other than the one its tier
/// names: a debug engine that confines nothing among them.
pub const REASON_ENGINE_NOT_THE_TIERS: &str = "the runner's engine is not its tier's";

/// The engine a tier names and the wall that engine must report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wall {
    /// The engine that builds the tier's sandboxes.
    engine: SandboxEngine,
    /// The isolation that engine puts between a sandbox and the host.
    isolation: Isolation,
}

/// The wall `tier` demands, or none for a tier that builds no sandbox.
const fn wall_of(tier: SandboxTier) -> Option<Wall> {
    match tier {
        SandboxTier::Firecracker => Some(Wall {
            engine: SandboxEngine::Firecracker,
            isolation: Isolation::HardwareVm,
        }),
        SandboxTier::DevNone => None,
    }
}

/// Whether a runner may take work, and why not when it may not.
///
/// An enum rather than a `bool` beside an `Option<&str>`, so the two illegal
/// combinations that pair can spell are unrepresentable here.
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

/// Resolves the assignment against what the host reported it runs.
///
/// A tier that builds no sandbox demands nothing of any host, and of a host
/// that has never reported, but it cannot hold a network posture other than
/// `allow_all`: the child shares the host's network namespace. A tier that
/// names an engine needs a report naming that engine behind its wall.
#[must_use]
pub fn reconcile(
    policy: &AssignedPolicy<'_>,
    achievable: Option<&CapabilityReport<'_>>,
) -> Verdict {
    let Some(wall) = wall_of(policy.sandbox_tier) else {
        return if policy.network_policy == NetworkPolicy::AllowAll {
            Verdict::Healthy
        } else {
            Verdict::degraded(REASON_NETWORK_NEEDS_CAGE)
        };
    };
    let Some(report) = achievable else {
        return Verdict::degraded(REASON_NO_CAPABILITY_REPORT);
    };
    if report.engine == wall.engine && report.isolation == wall.isolation {
        Verdict::Healthy
    } else {
        Verdict::degraded(REASON_ENGINE_NOT_THE_TIERS)
    }
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
