use af_wire::runner::{Isolation, Kvm, NetworkPolicy, SandboxEngine, SandboxTier};
use afr_engine::{Cap, Capabilities, HostProbe, REQUIRED_CONTROLLERS, ResourceFacts};

use super::{CGROUP_OFF, CGROUP_ON, capability_report, probe_answer, selftest};

/// An engine that confines nothing, as the debug engine states itself.
const UNCONFINED: Capabilities = Capabilities {
    engine: SandboxEngine::Unsandboxed,
    isolation: Isolation::Unconfined,
    parks: &[],
    snapshots: false,
    resources: ResourceFacts {
        memory: Cap::Absent,
        cpu: Cap::Absent,
        pids: Cap::Absent,
        disk: Cap::Absent,
    },
};

/// An engine behind a virtual machine, which relies on the KVM device.
const MICROVM: Capabilities = Capabilities {
    engine: SandboxEngine::Firecracker,
    isolation: Isolation::HardwareVm,
    parks: &[],
    snapshots: false,
    resources: ResourceFacts::ENFORCED,
};

fn probe(kvm: Kvm, toolbox_filesystem: bool) -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec![
            "cpu".to_owned(),
            "memory".to_owned(),
            "pids".to_owned(),
            "io".to_owned(),
        ],
        kvm,
        toolbox_filesystem,
        workspace_direct_io: None,
        egress: false,
    }
}

fn check(report: &af_wire::runner::SelftestReport<'_>, name: &str) -> (bool, String) {
    report
        .checks
        .iter()
        .find(|check| check.name == name)
        .map_or_default(|check| (check.ok, check.detail.to_string()))
}

#[test]
fn test_capability_report_states_kvm_and_toolbox_fs() {
    let usable = selftest(
        &probe(Kvm::Usable, true),
        &MICROVM,
        SandboxTier::Firecracker,
        NetworkPolicy::AllowAll,
    );
    let denied = selftest(
        &probe(Kvm::Denied, true),
        &MICROVM,
        SandboxTier::Firecracker,
        NetworkPolicy::AllowAll,
    );
    let absent = selftest(
        &probe(Kvm::Absent, false),
        &MICROVM,
        SandboxTier::DevNone,
        NetworkPolicy::DenyAllEgress,
    );

    assert!(check(&usable, "kvm").0);
    assert!(check(&usable, "kvm").1.contains("microVM engine can run"));
    assert!(!check(&denied, "kvm").0 && check(&denied, "kvm").1.contains("cannot open"));
    assert!(!check(&absent, "kvm").0 && check(&absent, "kvm").1.contains("no KVM device"));
    assert!(check(&usable, "toolbox_filesystem").0);
    assert!(!check(&absent, "toolbox_filesystem").0);
    assert!(
        check(&absent, "toolbox_filesystem")
            .1
            .contains("no sandbox can be built")
    );
    assert!(usable.all_ok);
    assert!(
        !denied.all_ok && !absent.all_ok,
        "all_ok agrees with its checks"
    );
    assert_eq!(
        (usable.sandbox_tier.as_ref(), usable.network_policy.as_ref()),
        ("firecracker", "allow_all")
    );
    assert_eq!(absent.network_policy, "deny_all_egress");
}

#[test]
fn every_missing_mechanism_fails_its_own_check() {
    let bare = HostProbe {
        landlock: false,
        seccomp: false,
        cgroup_controllers: vec!["cpu".to_owned()],
        kvm: Kvm::Absent,
        toolbox_filesystem: false,
        workspace_direct_io: None,
        egress: false,
    };

    let report = selftest(
        &bare,
        &MICROVM,
        SandboxTier::DevNone,
        NetworkPolicy::AllowAll,
    );

    assert!(report.checks.iter().all(|check| !check.ok));
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.detail.chars().count() <= af_validate::TEXT_MAX_CHARS)
    );
}

/// The report names the engine, its wall and what `/dev/kvm` allows, and
/// no host mechanism.
#[test]
fn the_report_names_the_engine_and_its_wall() {
    let probe = probe(Kvm::Usable, true);

    let report = capability_report(&probe, &MICROVM, Some(3));

    assert_eq!(report.engine, SandboxEngine::Firecracker);
    assert_eq!(report.isolation, Isolation::HardwareVm);
    assert_eq!(report.kvm, Kvm::Usable);
    assert_eq!(report.sandboxes_max, Some(3));
}

/// The probe command answers with the heartbeat's report and every check, so
/// an operator reads on the host exactly what the daemon will be told.
#[test]
fn the_probe_answer_is_the_report_plus_every_check() {
    let host = probe(Kvm::Absent, false);

    let answer = probe_answer(&host, &MICROVM);

    assert_eq!(
        answer.capability_report,
        capability_report(&host, &MICROVM, None)
    );
    let names: Vec<_> = answer
        .checks
        .iter()
        .map(|check| check.name.as_ref())
        .collect();
    assert_eq!(
        names,
        [
            "landlock",
            "seccomp",
            "cgroup_controllers",
            "kvm",
            "toolbox_filesystem"
        ]
    );
    let refused: Vec<_> = answer
        .checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.name.as_ref())
        .collect();
    assert_eq!(refused, ["kvm", "toolbox_filesystem"]);
}

/// A host is judged only on what its engine relies on: the KVM device for an
/// engine behind a virtual machine, and not for the unsandboxed engine.
#[test]
fn the_checks_are_only_those_the_engine_relies_on() {
    let host = probe(Kvm::Usable, true);
    let checked = |engine: &Capabilities| -> Vec<String> {
        probe_answer(&host, engine)
            .checks
            .iter()
            .map(|check| check.name.to_string())
            .collect()
    };

    for (engine, kvm) in [(MICROVM, true), (UNCONFINED, false)] {
        let named = checked(&engine);
        assert_eq!(named.iter().any(|name| name == "kvm"), kvm, "{named:?}");
    }
}

/// The unsandboxed engine needs no KVM device: a host without it passes its
/// self-test, while the same host under an engine behind a virtual machine
/// fails on the KVM check alone.
#[test]
fn a_host_without_kvm_fails_only_under_a_microvm_engine() {
    let tier = SandboxTier::Firecracker;
    let network = NetworkPolicy::AllowAll;

    let unconfined = selftest(&probe(Kvm::Absent, true), &UNCONFINED, tier, network);
    let microvm = selftest(&probe(Kvm::Denied, true), &MICROVM, tier, network);

    assert!(unconfined.all_ok, "{:?}", unconfined.checks);
    assert!(!microvm.all_ok);
    let failing: Vec<_> = microvm
        .checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.name.as_ref())
        .collect();
    assert_eq!(failing, ["kvm"]);
    let (_, detail) = check(&microvm, "kvm");
    assert_eq!(detail, super::KVM_DENIED);
}

#[test]
fn test_direct_io_check_follows_the_probe() {
    let stated = |workspace_direct_io| {
        let probe = HostProbe {
            workspace_direct_io,
            egress: false,
            ..probe(Kvm::Usable, true)
        };
        selftest(
            &probe,
            &MICROVM,
            SandboxTier::Firecracker,
            NetworkPolicy::AllowAll,
        )
    };

    let unnamed = stated(None);
    let direct = stated(Some(true));
    let buffered = stated(Some(false));

    assert!(
        unnamed
            .checks
            .iter()
            .all(|check| check.name != "workspace_direct_io"),
        "no state directory, no claim about it"
    );
    assert!(check(&direct, "workspace_direct_io").0);
    assert!(
        check(&direct, "workspace_direct_io")
            .1
            .contains("cached once")
    );
    let (ok, detail) = check(&buffered, "workspace_direct_io");
    assert!(!ok && detail.contains("falls back to buffered"));
}

/// The cgroup check's two sentences name every controller the sandbox
/// requires, so the prose cannot drift from the set it reports on.
#[test]
fn the_cgroup_check_names_every_required_controller() {
    for sentence in [CGROUP_ON, CGROUP_OFF] {
        let words: Vec<&str> = sentence
            .split(|character: char| !character.is_ascii_alphanumeric())
            .collect();
        for controller in REQUIRED_CONTROLLERS {
            assert!(words.contains(&controller), "{sentence} names {controller}");
        }
    }
}
