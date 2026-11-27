use af_wire::runner::{
    AssignedPolicy, CapabilityReport, Isolation, Kvm, NetworkPolicy, SandboxEngine, SandboxTier,
};

use super::{REASON_ENGINE_NOT_THE_TIERS, Verdict, reconcile};

/// A host whose engine is `engine` behind `isolation`.
const fn report(engine: SandboxEngine, isolation: Isolation) -> CapabilityReport<'static> {
    CapabilityReport {
        durable_tasks_v1: true,
        build_id: None,
        engine,
        isolation,
        kvm: Kvm::Usable,
        sandboxes_max: None,
    }
}

const fn assigned(tier: SandboxTier, network_policy: NetworkPolicy) -> AssignedPolicy<'static> {
    AssignedPolicy {
        durable_tasks_enabled: false,
        decision_model_id: None,
        sandbox_tier: tier,
        network_policy,
        registry_allowlist: Vec::new(),
        worker_count: 1,
        extra_binds: Vec::new(),
    }
}

/// A `firecracker` tier is proven by a Firecracker report behind a hardware
/// virtual machine, under every network posture; a report naming the debug
/// engine, or a Firecracker report claiming another wall, reads degraded with
/// the mismatch's own reason; `dev_none` accepts the debug engine.
#[test]
fn test_a_tier_names_its_engine() {
    let vm = report(SandboxEngine::Firecracker, Isolation::HardwareVm);
    let unsandboxed = report(SandboxEngine::Unsandboxed, Isolation::Unconfined);
    let lying = report(SandboxEngine::Firecracker, Isolation::Unconfined);
    let mismatch = Verdict::Degraded {
        reason: REASON_ENGINE_NOT_THE_TIERS,
    };

    for network in [
        NetworkPolicy::AllowAll,
        NetworkPolicy::DenyAllEgress,
        NetworkPolicy::AllowListEgress,
    ] {
        let firecracker = assigned(SandboxTier::Firecracker, network);
        assert_eq!(reconcile(&firecracker, Some(&vm)), Verdict::Healthy);
        assert_eq!(reconcile(&firecracker, Some(&unsandboxed)), mismatch);
        assert_eq!(reconcile(&firecracker, Some(&lying)), mismatch);
    }
    let dev = assigned(SandboxTier::DevNone, NetworkPolicy::AllowAll);
    assert_eq!(reconcile(&dev, Some(&unsandboxed)), Verdict::Healthy);
}
