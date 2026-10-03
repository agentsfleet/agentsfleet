//! Releasing what a lease holds: on teardown, on drop, and at boot.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;
use std::time::Duration;

use afd_core::test_util::trace::Capture;

use super::super::parts::Parts;

/// How long a test waits for a release handed to the blocking pool.
const PATIENCE: Duration = Duration::from_secs(10);
/// How often it looks.
const POLL: Duration = Duration::from_millis(10);
use super::support::{FakeHost, IMAGE, SLEEPER, request};
use crate::engine::Engine;

#[tokio::test]
async fn test_parts_with_no_sandbox_never_report_an_exit() {
    let dir = tempfile::tempdir().unwrap();
    let mut parts = Parts::new("lease-1", dir.path().to_owned());

    let waited = tokio::time::timeout(Duration::from_millis(5), parts.exited()).await;

    waited.unwrap_err();
    assert!(!parts.is_running());
    parts.teardown().await.unwrap();
    assert!(
        !dir.path().exists(),
        "an empty lease's directory is all teardown removes"
    );
}

#[test]
fn test_dropping_parts_releases_what_they_hold() {
    let dir = tempfile::tempdir().unwrap();
    let lease = dir.path().join("lease-2");
    fs::create_dir(&lease).unwrap();

    drop(Parts::new("lease-2", lease.clone()));

    assert!(!lease.exists());
}

/// On a runtime, even a single-threaded one with no sibling worker to take
/// over, the release goes to the blocking pool rather than holding the thread
/// that dropped the parts.
#[tokio::test]
async fn test_dropping_parts_on_a_runtime_releases_them_off_its_workers() {
    let dir = tempfile::tempdir().unwrap();
    let lease = dir.path().join("lease-6");
    fs::create_dir(&lease).unwrap();

    drop(Parts::new("lease-6", lease.clone()));

    tokio::time::timeout(PATIENCE, async {
        while lease.exists() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap();
}

/// A start abandoned mid-way — its caller stopped waiting — is released when
/// its parts drop: the launcher is killed and what could not go is logged.
#[tokio::test]
async fn test_a_cancelled_start_releases_its_sandbox() {
    let host = FakeHost::new(SLEEPER);
    let engine = host.engine();
    let capture = Capture::install();

    let abandoned = tokio::time::timeout(
        Duration::from_millis(100),
        engine.prepare(request("lease-3")),
    )
    .await;

    abandoned.unwrap_err();
    // Released on the blocking pool, logging where this test listens.
    tokio::time::timeout(PATIENCE, async {
        while capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some("sandbox_teardown_failed"))
        {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        capture.only("sandbox_teardown_failed").field("lease_id"),
        Some("lease-3")
    );
    assert!(
        host.lease_dir("lease-3").join(IMAGE).exists(),
        "kept, never unlinked"
    );
}

#[test]
fn test_the_boot_sweep_removes_what_a_previous_run_left() {
    let host = FakeHost::new(SLEEPER);
    let clean = host.lease_dir("lease-4");
    fs::create_dir_all(clean.join("workspace")).unwrap();
    fs::write(clean.join(IMAGE), "").unwrap();
    let busy = host.lease_dir("lease-5");
    fs::create_dir_all(&busy).unwrap();
    fs::create_dir_all(host.config.cgroup_root.join("lease-5").join("child")).unwrap();
    fs::write(host.dir.path().join("leases").join("stray-file"), "").unwrap();
    let capture = Capture::install();

    let _engine = host.engine();

    assert!(!clean.exists());
    assert_eq!(
        capture.only("sandbox_swept").field("lease_id"),
        Some("lease-4")
    );
    assert!(
        busy.exists(),
        "a cgroup that will not go keeps its lease for later"
    );
    assert_eq!(
        capture.only("sandbox_sweep_failed").field("lease_id"),
        Some("lease-5")
    );
}

/// What runs in the child between fork and exec, run here where it can be read.
#[test]
fn test_entering_a_cgroup_writes_this_process_into_it() {
    let dir = tempfile::tempdir().unwrap();
    let procs = dir.path().join("cgroup.procs");
    let file = fs::File::create(&procs).unwrap();

    super::super::parts::enter(&file).unwrap();

    assert_eq!(fs::read_to_string(&procs).unwrap(), "0");
}
