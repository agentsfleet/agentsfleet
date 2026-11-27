//! The verdict matrix: every way an assignment and a host can disagree.
//!
//! Pure, so the whole matrix runs with no datastore, no clock and no runtime.
//! That is the property the split exists for — the heartbeat only orchestrates
//! reads and writes around this function, so this is where the decision is
//! proven and the integration lane proves only that the right rows were written
//! with it.
use af_wire::runner::{
    AssignedPolicy, CapabilityReport, Isolation, Kvm, NetworkPolicy, SandboxEngine, SandboxTier,
};
use afd_runner::Verdict;
use afd_runner::reconcile::{REASON_NETWORK_NEEDS_CAGE, REASON_NO_CAPABILITY_REPORT, reconcile};

/// An assignment at `tier` under `network`, with nothing else set.
const fn assigned(tier: SandboxTier, network: NetworkPolicy) -> AssignedPolicy<'static> {
    AssignedPolicy {
        decision_model_id: None,
        durable_tasks_enabled: af_wire::test_util::DURABLE_TASKS_DEFAULT_ENABLED,
        sandbox_tier: tier,
        network_policy: network,
        registry_allowlist: Vec::new(),
        worker_count: 1,
        extra_binds: Vec::new(),
    }
}

/// A Firecracker host behind its microVM wall.
const fn capable() -> CapabilityReport<'static> {
    CapabilityReport {
        durable_tasks_v1: false,
        build_id: None,
        engine: SandboxEngine::Firecracker,
        isolation: Isolation::HardwareVm,
        kvm: Kvm::Usable,
        sandboxes_max: None,
    }
}

/// The reason a verdict carries, or a sentence saying it carried none.
fn reason_of(verdict: Verdict) -> &'static str {
    verdict.reason().unwrap_or("<healthy>")
}

/// A cage tier that has never reported is degraded, not optimistically healthy.
///
/// This is the fail-CLOSED window: between minting a token and the host's first
/// beat there is no evidence, and a daemon that assumed the best would hand out
/// runs to a host that cannot cage them.
#[test]
fn test_a_cage_tier_with_no_report_is_degraded() {
    let policy = assigned(SandboxTier::Firecracker, NetworkPolicy::AllowAll);

    let verdict = reconcile(&policy, None);

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

    assert_eq!(reconcile(&policy, None), Verdict::Healthy);
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

        let verdict = reconcile(&policy, Some(&capable()));

        assert_eq!(
            reason_of(verdict),
            REASON_NETWORK_NEEDS_CAGE,
            "{network:?} on a cage-less tier is unachievable, not merely unproven"
        );
    }
}
