#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::Path;

use super::{LeaseCgroup, drained};
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
    assert_eq!(made.procs(), dir.join("cgroup.procs"));
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
fn test_io_is_limited_only_where_the_controller_is() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-3");

    made.limit_io((7, 3), 1_024).unwrap();
    assert!(
        !dir.join("io.max").exists(),
        "no io controller, no io limit"
    );

    fs::write(dir.join("cgroup.controllers"), "cpu io memory pids").unwrap();
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

#[tokio::test]
async fn test_kill_writes_one_and_remove_names_what_stopped_it() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-8");

    made.kill().unwrap();
    assert_eq!(read(&dir, "cgroup.kill"), "1");
    // Without the kernel the kill lands as a file, so the removal finds a
    // non-empty directory and says which control file it was working through.
    let left = made.remove().await.unwrap_err();

    assert!(left.to_string().contains("cgroup.procs"), "{left}");
}

#[tokio::test(start_paused = true)]
async fn test_remove_gives_up_on_a_cgroup_that_never_drains() {
    let root = tempfile::tempdir().unwrap();
    let (made, dir) = plain(root.path(), "lease-9");
    fs::write(dir.join(super::CGROUP_EVENTS), "populated 1\n").unwrap();

    let stuck = made.remove().await.unwrap_err();

    assert!(stuck.to_string().contains(super::CGROUP_EVENTS), "{stuck}");
}

#[test]
fn test_drained_reads_the_events_file() {
    let dir = tempfile::tempdir().unwrap();
    let events = dir.path().join(super::CGROUP_EVENTS);

    assert!(drained(&events), "no file, nothing to wait for");
    fs::write(&events, "populated 1\nfrozen 0\n").unwrap();
    assert!(!drained(&events));
    fs::write(&events, "populated 0\nfrozen 0\n").unwrap();
    assert!(drained(&events));
}
