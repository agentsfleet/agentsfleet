use std::borrow::Cow;

use af_wire::runner::{
    AssignedPolicy, CapabilityReport, Isolation, Kvm, NetworkPolicy, SandboxEngine, SandboxTier,
};

use super::{REASON_ENGINE_UNCONFINED, Verdict, reconcile};

/// A host whose engine is `engine` behind `isolation`, reporting no Linux
/// mechanism at all.
fn report(engine: SandboxEngine, isolation: Isolation) -> CapabilityReport<'static> {
    CapabilityReport {
        durable_tasks_v1: true,
        build_id: None,
        landlock: false,
        seccomp: false,
        cgroup_controllers: vec![Cow::Borrowed("cpu")],
        bubblewrap: false,
        egress_enforcement: true,
        engine,
        isolation,
        kvm: Kvm::Usable,
        sandboxes_max: None,
    }
}

fn cage(network_policy: NetworkPolicy) -> AssignedPolicy<'static> {
    AssignedPolicy {
        durable_tasks_enabled: false,
        decision_model_id: None,
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy,
        registry_allowlist: Vec::new(),
        worker_count: 1,
        extra_binds: Vec::new(),
    }
}

/// A virtual machine's wall proves the cage under `landlock_full`, with no
/// Linux mechanism reported; an engine that confines nothing reads degraded
/// with its own reason, whatever the host offers.
#[test]
fn test_a_hardware_vm_report_proves_the_cage_guarantees() {
    let vm = report(SandboxEngine::Firecracker, Isolation::HardwareVm);
    let mut unconfined = report(SandboxEngine::Unsandboxed, Isolation::Unconfined);
    unconfined.landlock = true;
    unconfined.seccomp = true;
    unconfined.bubblewrap = true;

    assert_eq!(
        reconcile(&cage(NetworkPolicy::AllowAll), Some(&vm)),
        Verdict::Healthy
    );
    assert_eq!(
        reconcile(&cage(NetworkPolicy::AllowAll), Some(&unconfined)),
        Verdict::Degraded {
            reason: REASON_ENGINE_UNCONFINED
        }
    );
}
