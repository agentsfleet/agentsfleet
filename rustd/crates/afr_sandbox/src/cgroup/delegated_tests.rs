#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::path::Path;

use super::{MECHANISM_DELEGATED_CGROUP, delegated_root};

/// Where cgroup v2 is mounted on every host the runner supports.
const MOUNT: &str = "/sys/fs/cgroup";

fn root_of(own: &str) -> crate::Result<std::path::PathBuf> {
    delegated_root(own.as_bytes(), Path::new(MOUNT))
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
    for own in ["0::/\n", "0::/runner\n", "4:memory:/x/y\n", ""] {
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
