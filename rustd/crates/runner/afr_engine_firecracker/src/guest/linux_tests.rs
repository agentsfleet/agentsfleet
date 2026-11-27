#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;

use afr_cgroup::{CGROUP_PROCS, MEMORY_EVENTS, PIDS_EVENTS, PIDS_MAX, SUBTREE_CONTROL};

use super::{leaf, tenant};

/// The leaf every tenant process is placed in sits under the guest's cgroup
/// root, which hands it the memory and pids controllers; its process cap is
/// the one the entry was told, and the executor takes it as its tenant from
/// the three files the kernel publishes there.
#[test]
fn every_tenant_process_is_placed_in_a_leaf_whose_memory_is_counted_and_processes_capped() {
    let root = tempfile::tempdir().unwrap();

    let made = leaf(root.path(), 37).unwrap();

    assert_eq!(made, root.path().join("tenant"));
    assert!(made.is_dir(), "the leaf is made");
    assert_eq!(
        fs::read_to_string(root.path().join(SUBTREE_CONTROL)).unwrap(),
        "+memory +pids"
    );
    assert_eq!(
        fs::read_to_string(made.join(PIDS_MAX)).unwrap(),
        "37",
        "the tenant leaf holds the run's process cap"
    );
    fs::write(made.join(CGROUP_PROCS), "").unwrap();
    fs::write(made.join(MEMORY_EVENTS), "oom 0\noom_kill 0\n").unwrap();
    fs::write(made.join(PIDS_EVENTS), "max 0\n").unwrap();
    tenant(&made).unwrap();
}
