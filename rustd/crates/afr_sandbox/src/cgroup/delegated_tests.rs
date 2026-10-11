#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::error::Error as _;
use std::fs;
use std::path::Path;

use super::{
    CONTROLLERS, MECHANISM_DELEGATED_CGROUP, SUBTREE_CONTROL, delegated_root, enable_controllers,
};

/// Where cgroup v2 is mounted on every host the runner supports.
const MOUNT: &str = "/sys/fs/cgroup";

fn root_of(own: &str) -> crate::Result<std::path::PathBuf> {
    delegated_root(own.as_bytes(), Path::new(MOUNT), "runner")
}

/// Under systemd's `DelegateSubgroup=runner`, the runner sits in `runner` and
/// its leases go beside it, in the service's cgroup.
#[test]
fn test_the_delegated_root_is_the_parent_of_the_leaf_the_process_runs_in() {
    let root = root_of("0::/system.slice/agentsfleet-runner.service/runner\n").unwrap();

    assert_eq!(
        root,
        Path::new("/sys/fs/cgroup/system.slice/agentsfleet-runner.service")
    );
}

/// A host still mounting a v1 hierarchy beside v2 lists it first; only the
/// unified line names where leases go.
#[test]
fn test_a_v1_hierarchy_beside_the_unified_one_is_ignored() {
    let own = "12:pids:/system.slice/other.service\n0::/a.slice/b.service/runner\n";

    assert_eq!(
        root_of(own).unwrap(),
        Path::new("/sys/fs/cgroup/a.slice/b.service")
    );
}

/// A process at or just under the top of the tree has no delegation: its
/// parent would be the whole host, so it is refused, as is a host with no
/// unified hierarchy at all.
#[test]
fn test_a_process_with_no_delegated_parent_is_refused() {
    for own in [
        "0::/\n",
        "0::/runner\n",
        "4:memory:/x/y\n",
        "",
        "0::/system.slice/agentsfleet-runner.service\n",
        "0::/system.slice/agentsfleet-runner.service/unexpected\n",
    ] {
        let refused = root_of(own).unwrap_err();

        assert_eq!(
            refused.missing_mechanism(),
            Some(MECHANISM_DELEGATED_CGROUP),
            "{own:?}"
        );
    }
}

/// Text that is not the kernel's format is an input failure, not a host that
/// lacks delegation.
#[test]
fn test_unparseable_text_is_an_input_failure() {
    let refused = root_of("not a cgroup line\n").unwrap_err();

    assert_eq!(refused.missing_mechanism(), None, "{refused}");
}

#[test]
fn test_delegated_controllers_are_enabled_with_only_positive_commands() {
    for initial in ["", "cpu memory"] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(CONTROLLERS), "cpuset cpu io memory pids\n").unwrap();
        fs::write(root.path().join(SUBTREE_CONTROL), initial).unwrap();

        enable_controllers(root.path()).unwrap();

        assert_eq!(
            fs::read_to_string(root.path().join(SUBTREE_CONTROL)).unwrap(),
            "+cpu +io +memory +pids"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}

#[test]
fn test_each_unavailable_controller_refuses_before_activation() {
    for (available, missing) in [
        ("cpuset io memory pids", "cpu"),
        ("cpu memory pids", "io"),
        ("cpu io pids", "memory"),
        ("cpu io memory", "pids"),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(CONTROLLERS), available).unwrap();
        fs::write(root.path().join(SUBTREE_CONTROL), "").unwrap();

        let error = enable_controllers(root.path()).unwrap_err();

        assert_eq!(error.missing_mechanism(), Some(missing));
        assert_eq!(
            fs::read_to_string(root.path().join(SUBTREE_CONTROL)).unwrap(),
            ""
        );
    }
}

#[test]
fn test_unreadable_controller_availability_keeps_the_cause() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(SUBTREE_CONTROL), "").unwrap();

    let error = enable_controllers(root.path()).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cgroup.controllers could not be read")
    );
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        fs::read_to_string(root.path().join(SUBTREE_CONTROL)).unwrap(),
        ""
    );
}

#[test]
fn test_activation_does_not_create_a_missing_control_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(CONTROLLERS), "cpu io memory pids").unwrap();

    let error = enable_controllers(root.path()).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("refused a write to cgroup.subtree_control")
    );
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
    assert!(!root.path().join(SUBTREE_CONTROL).exists());
}

#[test]
fn test_activation_write_failure_keeps_the_cause() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(CONTROLLERS), "cpu io memory pids").unwrap();
    fs::create_dir(root.path().join(SUBTREE_CONTROL)).unwrap();

    let error = enable_controllers(root.path()).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("refused a write to cgroup.subtree_control")
    );
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::IsADirectory);
    assert!(root.path().join(SUBTREE_CONTROL).is_dir());
}
