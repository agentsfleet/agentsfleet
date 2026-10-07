#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::Path;

use super::LeaseCgroup;
use crate::engine::Limits;

const LIMITS: Limits = Limits {
    memory_bytes: 268_435_456,
    cpu_millis: 1_500,
    pids: 64,
    disk_bytes: 67_108_864,
};

fn read(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap()
}

/// A cgroup over a plain directory, which takes every write.
fn plain(root: &Path, name: &str) -> (LeaseCgroup, std::path::PathBuf) {
    let dir = root.join(name);
    fs::create_dir(&dir).unwrap();
    (LeaseCgroup { dir: dir.clone() }, dir)
}

#[test]
fn test_lease_cgroup_writes_every_limit() {
    let root = tempfile::tempdir().unwrap();

    let made = LeaseCgroup::create(root.path(), "lease-1", &LIMITS).unwrap();

    let dir = root.path().join("lease-1");
    assert_eq!(read(&dir, "memory.max"), "268435456");
    // pin test: literal is the contract
    assert_eq!(read(&dir, "cpu.max"), "150000 100000");
    assert_eq!(read(&dir, "pids.max"), "64");
    assert!(
        !dir.join("memory.swap.max").exists(),
        "no swap accounting, nothing written"
    );
    assert_eq!(made.procs(), dir.join("sandbox").join("cgroup.procs"));
}

#[test]
fn test_a_lease_cgroup_splits_into_a_sandbox_leaf_and_a_smaller_tenant_leaf() {
    let root = tempfile::tempdir().unwrap();

    let made = LeaseCgroup::create(root.path(), "lease-9", &LIMITS).unwrap();

    let dir = root.path().join("lease-9");
    assert_eq!(
        read(&dir, "cgroup.subtree_control"),
        "+cpu +io +memory +pids"
    );
    assert!(dir.join("sandbox").is_dir(), "bubblewrap's leaf");
    let tenant = dir.join("tenant");
    assert_eq!(
        read(&tenant, "memory.max"),
        (LIMITS.memory_bytes - super::SANDBOX_MEMORY_RESERVE_BYTES).to_string(),
        "the tenant runs out before the sandbox"
    );
    assert_eq!(made.tenant_procs(), tenant.join("cgroup.procs"));
    assert_eq!(made.tenant_events(), tenant.join("memory.events"));
}

#[test]
fn test_a_limit_below_the_reserve_leaves_the_tenant_nothing_rather_than_wrapping() {
    let root = tempfile::tempdir().unwrap();
    let tiny = Limits {
        memory_bytes: 1_024,
        ..LIMITS
    };

    LeaseCgroup::create(root.path(), "lease-10", &tiny).unwrap();

    assert_eq!(
        read(&root.path().join("lease-10/tenant"), "memory.max"),
        "0"
    );
}

#[test]
fn test_removal_takes_both_leaves_before_the_lease_cgroup() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-11");
    fs::create_dir(dir.join("sandbox")).unwrap();
    fs::create_dir(dir.join("tenant")).unwrap();

    // A directory with children cannot go, so the lease going at all says
    // its leaves went first.
    made.remove_dirs().unwrap();

    assert!(!dir.exists(), "leaves, then the lease");
}

#[test]
fn test_a_crash_before_the_split_leaves_nothing_the_removal_trips_on() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-12");

    made.remove_dirs().unwrap();

    assert!(!dir.exists(), "no leaves, the lease goes alone");
}

#[test]
fn test_swap_is_zeroed_where_the_kernel_accounts_it() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-2");
    fs::write(dir.join("memory.swap.max"), "max").unwrap();

    made.limit(&LIMITS).unwrap();

    assert_eq!(read(&dir, "memory.swap.max"), "0");
}

#[test]
fn test_io_is_limited_on_the_workspace_device() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-3");

    made.limit_io((7, 3), 1_024).unwrap();
    // pin test: literal is the contract
    assert_eq!(read(&dir, "io.max"), "7:3 rbps=1024 wbps=1024");
}

#[test]
fn test_a_cgroup_under_a_missing_root_is_refused() {
    let root = tempfile::tempdir().unwrap();

    let refused = LeaseCgroup::create(&root.path().join("absent"), "lease-4", &LIMITS);

    refused.unwrap_err();
}

#[test]
fn test_a_refused_limit_names_its_control_file() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-5");
    fs::create_dir(dir.join("pids.max")).unwrap();

    let refused = made.limit(&LIMITS).unwrap_err();

    assert!(refused.to_string().contains("pids.max"), "{refused}");
}

#[test]
fn test_undo_removes_an_empty_cgroup_and_logs_one_it_cannot() {
    let root = tempfile::tempdir().unwrap();
    let capture = afd_core::test_util::trace::Capture::install();
    let (empty, empty_dir) = plain(root.path(), "lease-6");
    let (full, full_dir) = plain(root.path(), "lease-7");
    fs::write(full_dir.join("memory.max"), "1").unwrap();

    let first = empty.undo(crate::error::unconfined("first"));
    let second = full.undo(crate::error::unconfined("second"));

    assert!(!empty_dir.exists(), "an empty cgroup is removed");
    assert!(first.to_string().contains("first") && second.to_string().contains("second"));
    assert_eq!(
        capture.only("sandbox_cgroup_left").field("event"),
        Some("sandbox_cgroup_left")
    );
}

#[test]
fn test_kill_writes_one_and_remove_names_the_cgroup_that_stayed() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-8");

    made.kill().unwrap();
    assert_eq!(read(&dir, "cgroup.kill"), "1");
    // Without the kernel the kill lands as a file, so the removal finds a
    // non-empty directory: refused at once, not retried, and named.
    let left = made.remove().unwrap_err();

    assert!(left.to_string().contains("lease-8"), "{left}");
    assert!(left.to_string().contains("could not be removed"), "{left}");
}
