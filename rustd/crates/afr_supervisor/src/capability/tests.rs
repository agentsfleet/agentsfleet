use afd_wire::runner::{NetworkPolicy, SandboxTier};
use afr_sandbox::{HostProbe, Kvm};

use super::{capability_report, probe_answer, selftest};

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
        bubblewrap: true,
        kvm,
        toolbox_filesystem,
        workspace_direct_io: None,
    }
}

fn check(report: &afd_wire::runner::SelftestReport<'_>, name: &str) -> (bool, String) {
    report
        .checks
        .iter()
        .find(|check| check.name == name)
        .map(|check| (check.ok, check.detail.to_string()))
        .unwrap_or_default()
}

#[test]
fn test_capability_report_states_kvm_and_toolbox_fs() {
    let usable = selftest(
        &probe(Kvm::Usable, true),
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowAll,
    );
    let denied = selftest(
        &probe(Kvm::Denied, true),
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowAll,
    );
    let absent = selftest(
        &probe(Kvm::Absent, false),
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
        ("landlock_full", "allow_all")
    );
    assert_eq!(absent.network_policy, "deny_all_egress");
}

#[test]
fn every_missing_mechanism_fails_its_own_check() {
    let bare = HostProbe {
        landlock: false,
        seccomp: false,
        cgroup_controllers: vec!["cpu".to_owned()],
        bubblewrap: false,
        kvm: Kvm::Absent,
        toolbox_filesystem: false,
        workspace_direct_io: None,
    };

    let report = selftest(&bare, SandboxTier::DevNone, NetworkPolicy::AllowAll);

    assert!(report.checks.iter().all(|check| !check.ok));
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.detail.len() <= afd_wire::runner::CHECK_DETAIL_MAX_BYTES)
    );
}

#[test]
fn the_report_carries_the_wire_mechanisms_without_egress() {
    let probe = probe(Kvm::Usable, true);

    let report = capability_report(&probe);

    assert!(report.landlock && report.seccomp && report.bubblewrap);
    assert!(
        !report.egress_enforcement,
        "a sandbox here has no network to enforce"
    );
    assert_eq!(report.cgroup_controllers.len(), 4);
}

/// The probe command answers with the heartbeat's report and every check, so
/// an operator reads on the host exactly what the daemon will be told.
#[test]
fn the_probe_answer_is_the_report_plus_every_check() {
    let host = probe(Kvm::Absent, false);

    let answer = probe_answer(&host);

    assert_eq!(answer.capability_report, capability_report(&host));
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
            "bubblewrap",
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

#[test]
fn test_direct_io_check_follows_the_probe() {
    let stated = |workspace_direct_io| {
        let probe = HostProbe {
            workspace_direct_io,
            ..probe(Kvm::Usable, true)
        };
        selftest(&probe, SandboxTier::LandlockFull, NetworkPolicy::AllowAll)
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
