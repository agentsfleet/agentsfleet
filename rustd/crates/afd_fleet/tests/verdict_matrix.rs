//! The verdict matrix: every way an assignment and a host can disagree.
//!
//! Pure, so the whole matrix runs with no datastore, no clock and no runtime.
//! That is the property the split exists for — the heartbeat only orchestrates
//! reads and writes around this function, so this is where the decision is
//! proven and the integration lane proves only that the right rows were written
//! with it.
//!
//! # Why the ORDER is asserted and not just the outcome
//!
//! An operator fixing a degraded host reads one reason, fixes it, and beats
//! again. If the order were unstable they would see a different mechanism each
//! time and could not tell progress from churn. The order is therefore a
//! promise, and a promise nobody asserts is a comment.
use std::borrow::Cow;

use afd_fleet::Verdict;
use afd_fleet::runner::reconcile::{
    Guarantee, REASON_BUBBLEWRAP_MISSING, REASON_CGROUP_CONTROLLERS_MISSING,
    REASON_EGRESS_ENFORCEMENT_UNAVAILABLE, REASON_LANDLOCK_UNAVAILABLE, REASON_NETWORK_NEEDS_CAGE,
    REASON_NO_ASSIGNED_POLICY, REASON_NO_CAPABILITY_REPORT, REASON_SECCOMP_UNAVAILABLE, reconcile,
};
use afd_wire::runner::{AssignedPolicy, CapabilityReport, NetworkPolicy, SandboxTier};

/// An assignment at `tier` under `network`, with nothing else set.
fn assigned(tier: SandboxTier, network: NetworkPolicy) -> AssignedPolicy<'static> {
    AssignedPolicy {
        sandbox_tier: tier,
        network_policy: network,
        registry_allowlist: Vec::new(),
        worker_count: 1,
        extra_binds: Vec::new(),
    }
}

/// A host that can enforce everything.
fn capable() -> CapabilityReport<'static> {
    CapabilityReport {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec![
            Cow::Borrowed("cpu"),
            Cow::Borrowed("memory"),
            Cow::Borrowed("pids"),
        ],
        bubblewrap: true,
        egress_enforcement: true,
    }
}

/// The reason a verdict carries, or a sentence saying it carried none.
fn reason_of(verdict: Verdict) -> &'static str {
    verdict.reason().unwrap_or("<healthy>")
}

/// A row with no assignment degrades on its own, before anything is asked of
/// the host.
#[test]
fn test_no_assignment_degrades_before_the_host_is_consulted() {
    let verdict = reconcile(None, Some(&capable()));

    assert_eq!(reason_of(verdict), REASON_NO_ASSIGNED_POLICY);
}

/// A cage tier that has never reported is degraded, not optimistically healthy.
///
/// This is the fail-CLOSED window: between minting a token and the host's first
/// beat there is no evidence, and a daemon that assumed the best would hand out
/// leases to a host that cannot cage them.
#[test]
fn test_a_cage_tier_with_no_report_is_degraded() {
    let policy = assigned(SandboxTier::LandlockFull, NetworkPolicy::AllowAll);

    let verdict = reconcile(Some(&policy), None);

    assert_eq!(reason_of(verdict), REASON_NO_CAPABILITY_REPORT);
}

/// A tier that demands nothing is healthy even with no report at all.
///
/// Checked BEFORE the report, deliberately: a `dev_none` runner on `allow_all`
/// demands no guarantee, so waiting for a probe whose answer cannot change the
/// verdict would hold a usable host out of service for one beat.
#[test]
fn test_a_tier_that_demands_nothing_needs_no_report() {
    let policy = assigned(SandboxTier::DevNone, NetworkPolicy::AllowAll);

    assert_eq!(reconcile(Some(&policy), None), Verdict::Healthy);
}

/// An isolating posture on a cage-less tier is refused structurally.
///
/// Not a missing mechanism — no host build can deliver it. The child inherits
/// the host's network namespace, so there is nothing to enforce a posture on,
/// and rendering "no egress" over it would be a promise nothing keeps.
#[test]
fn test_an_isolating_posture_needs_a_tier_that_builds_a_cage() {
    for network in [NetworkPolicy::DenyAllEgress, NetworkPolicy::AllowListEgress] {
        let policy = assigned(SandboxTier::DevNone, network);

        let verdict = reconcile(Some(&policy), Some(&capable()));

        assert_eq!(
            reason_of(verdict),
            REASON_NETWORK_NEEDS_CAGE,
            "{network:?} on a cage-less tier is unachievable, not merely unproven"
        );
    }
}

/// A capable host under a full assignment takes work.
#[test]
fn test_a_capable_host_under_a_full_assignment_is_healthy() {
    for tier in [SandboxTier::LandlockFull, SandboxTier::ContainerNested] {
        let policy = assigned(tier, NetworkPolicy::AllowListEgress);

        assert_eq!(
            reconcile(Some(&policy), Some(&capable())),
            Verdict::Healthy,
            "{tier:?} with every guarantee proven must not be degraded"
        );
    }
}

/// Each unmet guarantee names its own reason, and the order is fixed.
///
/// Walked by removing one guarantee at a time from an otherwise capable host,
/// which is also the shape of the promise: fix the named mechanism, beat again,
/// see the next one — never a reshuffled answer.
#[test]
fn test_each_unmet_guarantee_names_itself_in_a_fixed_order() {
    let expected = [
        (Guarantee::FilesystemIsolation, REASON_LANDLOCK_UNAVAILABLE),
        (Guarantee::SyscallFiltering, REASON_SECCOMP_UNAVAILABLE),
        (Guarantee::ResourceLimits, REASON_CGROUP_CONTROLLERS_MISSING),
        (Guarantee::ProcessContainment, REASON_BUBBLEWRAP_MISSING),
        (
            Guarantee::EgressControl,
            REASON_EGRESS_ENFORCEMENT_UNAVAILABLE,
        ),
    ];
    let policy = assigned(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress);

    for (guarantee, reason) in expected {
        assert_eq!(
            guarantee.reason(),
            reason,
            "the operator-facing sentence for {guarantee:?} is pinned"
        );
        let host = without(guarantee);
        assert!(
            !guarantee.proven_by(&host),
            "the fixture must actually withhold {guarantee:?}"
        );
        assert_eq!(
            reason_of(reconcile(Some(&policy), Some(&host))),
            reason,
            "a host missing only {guarantee:?} must be told exactly that"
        );
    }
}

/// The refusal order holds when SEVERAL guarantees are unmet at once.
///
/// The first in the table wins, every time — which is what makes fixing them
/// one at a time terminate.
#[test]
fn test_the_first_unmet_guarantee_wins() {
    let policy = assigned(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress);
    let nothing = CapabilityReport {
        landlock: false,
        seccomp: false,
        cgroup_controllers: Vec::new(),
        bubblewrap: false,
        egress_enforcement: false,
    };

    assert_eq!(
        reason_of(reconcile(Some(&policy), Some(&nothing))),
        REASON_LANDLOCK_UNAVAILABLE,
        "a host missing everything is told the first thing, not the last"
    );
}

/// Egress control is demanded by the POSTURE, independent of the tier's own set.
#[test]
fn test_egress_control_is_demanded_by_the_posture_not_the_tier() {
    let host = without(Guarantee::EgressControl);

    // The same host, under the same cage tier, differing only in posture.
    let allowlisted = assigned(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress);
    let open = assigned(SandboxTier::LandlockFull, NetworkPolicy::AllowAll);

    assert_eq!(
        reason_of(reconcile(Some(&allowlisted), Some(&host))),
        REASON_EGRESS_ENFORCEMENT_UNAVAILABLE
    );
    assert_eq!(
        reconcile(Some(&open), Some(&host)),
        Verdict::Healthy,
        "an assignment that permits everything outbound demands no control over it"
    );
}

/// A partial controller set is not resource limits.
///
/// The cage needs every controller it enables, so two out of three is the same
/// answer as none — a lease that cannot be capped on processes is uncapped.
#[test]
fn test_a_partial_controller_set_does_not_prove_resource_limits() {
    let policy = assigned(SandboxTier::ContainerNested, NetworkPolicy::AllowAll);
    let partial = CapabilityReport {
        cgroup_controllers: vec![Cow::Borrowed("cpu"), Cow::Borrowed("memory")],
        ..capable()
    };

    assert_eq!(
        reason_of(reconcile(Some(&policy), Some(&partial))),
        REASON_CGROUP_CONTROLLERS_MISSING
    );
}

/// A host that proves everything except `guarantee`.
///
/// Written as a subtraction so a test names what is MISSING rather than
/// restating four fields that are not the subject.
fn without(guarantee: Guarantee) -> CapabilityReport<'static> {
    let mut host = capable();
    match guarantee {
        Guarantee::FilesystemIsolation => host.landlock = false,
        Guarantee::SyscallFiltering => host.seccomp = false,
        Guarantee::ResourceLimits => host.cgroup_controllers.clear(),
        Guarantee::ProcessContainment => host.bubblewrap = false,
        Guarantee::EgressControl => host.egress_enforcement = false,
    }
    host
}
