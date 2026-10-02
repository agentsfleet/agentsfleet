#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use super::{HostProbe, Kvm, ProbePaths, probe};

/// A host stated as files: every fact present unless a test removes it.
fn host(dir: &Path) -> ProbePaths {
    let paths = ProbePaths {
        kvm: dir.join("kvm"),
        filesystems: dir.join("filesystems"),
        lsm: dir.join("lsm"),
        seccomp_actions: dir.join("actions_avail"),
        cgroup_root: dir.join("cgroup"),
        bwrap: dir.join("bwrap"),
    };
    fs::write(&paths.kvm, "").unwrap();
    fs::write(&paths.filesystems, "nodev\tsysfs\n\text4\n\terofs\n").unwrap();
    fs::write(&paths.lsm, "capability,landlock,yama,bpf").unwrap();
    fs::write(
        &paths.seccomp_actions,
        "kill_process kill_thread trap errno allow",
    )
    .unwrap();
    fs::create_dir(&paths.cgroup_root).unwrap();
    fs::write(
        paths.cgroup_root.join("cgroup.subtree_control"),
        "cpu io memory pids\n",
    )
    .unwrap();
    fs::write(&paths.bwrap, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&paths.bwrap, fs::Permissions::from_mode(0o755)).unwrap();
    paths
}

#[test]
fn test_capability_probe_states_every_mechanism_it_finds() {
    let dir = tempfile::tempdir().unwrap();

    let found = probe(&host(dir.path()));

    assert_eq!(
        found,
        HostProbe {
            landlock: true,
            seccomp: true,
            cgroup_controllers: ["cpu", "io", "memory", "pids"].map(str::to_owned).to_vec(),
            bubblewrap: true,
            kvm: Kvm::Usable,
            toolbox_filesystem: true,
        }
    );
    assert_eq!(found.missing(), None);
}

#[test]
fn test_capability_probe_states_kvm_absent_and_denied() {
    let dir = tempfile::tempdir().unwrap();
    let paths = host(dir.path());

    fs::set_permissions(&paths.kvm, fs::Permissions::from_mode(0o000)).unwrap();
    let denied = probe(&paths).kvm;
    fs::remove_file(&paths.kvm).unwrap();
    let absent = probe(&paths).kvm;

    assert_eq!((denied, absent), (Kvm::Denied, Kvm::Absent));
}

#[test]
fn test_a_host_missing_a_mechanism_names_the_first_one() {
    let dir = tempfile::tempdir().unwrap();
    let paths = host(dir.path());
    let missing = |edit: &dyn Fn()| {
        edit();
        probe(&paths).missing()
    };

    assert_eq!(
        missing(&|| fs::write(&paths.filesystems, "\text4\n").unwrap()),
        Some("erofs")
    );
    assert_eq!(
        missing(&|| fs::write(
            paths.cgroup_root.join("cgroup.subtree_control"),
            "cpu memory"
        )
        .unwrap()),
        Some("erofs"),
        "the earlier gap is still reported first"
    );
    fs::write(&paths.filesystems, "\terofs\n").unwrap();
    assert_eq!(probe(&paths).missing(), Some("pids"));
    assert_eq!(
        missing(&|| fs::set_permissions(&paths.bwrap, fs::Permissions::from_mode(0o644)).unwrap()),
        Some("bubblewrap")
    );
    assert_eq!(
        missing(&|| fs::write(&paths.seccomp_actions, "allow").unwrap()),
        Some("seccomp")
    );
    assert_eq!(
        missing(&|| fs::write(&paths.lsm, "capability,yama").unwrap()),
        Some("landlock")
    );
}

#[test]
fn test_unreadable_facts_read_as_absent_mechanisms() {
    let dir = tempfile::tempdir().unwrap();
    let nowhere = dir.path().join("nowhere");
    let paths = ProbePaths {
        kvm: nowhere.clone(),
        filesystems: nowhere.clone(),
        lsm: nowhere.clone(),
        seccomp_actions: nowhere.clone(),
        cgroup_root: nowhere.clone(),
        bwrap: nowhere,
    };

    let found = probe(&paths);

    assert_eq!(found.missing(), Some("landlock"));
    assert!(found.cgroup_controllers.is_empty());
    assert_eq!(found.kvm, Kvm::Absent);
}

#[test]
fn test_default_paths_are_the_kernels_own() {
    let paths = ProbePaths::default();

    assert_eq!(paths.lsm, Path::new(super::LSM_PATH));
    assert_eq!(paths.kvm, Path::new(super::KVM_PATH));
    assert_eq!(paths.bwrap, Path::new(super::BWRAP_PATH));
}
