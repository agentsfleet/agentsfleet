//! Building a sandbox on the fake host: every way a start succeeds or refuses.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afr_executor::{Ending, Spawn};

use super::super::parts::{Parts, in_child};
use super::super::{BubblewrapEngine, EVENT_DISK_BUFFERED};
use super::support::{
    BRIEF, CLOSER, ENV_PROBE, ENV_PROBE_INHERITED, ENV_PROBE_REPORT, ENV_PROBE_VARIABLES, FAILER,
    FAILER_REASON, FALSE, FakeHost, IMAGE, LIMITS, POLL, SANDBOXED, SLEEPER, request, serve_leases,
};
use crate::cgroup::{CGROUP_PROCS, SANDBOX_LEAF};
use crate::engine::{Engine, Killed};
use crate::tenant::{Handover, TenantFiles};
use crate::warm_slots::WarmSlots;

#[tokio::test]
async fn test_a_lease_runs_through_the_engine_and_keeps_a_disk_it_cannot_unmount() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());

    let sandbox = host.engine().prepare(request("lease-1")).await.unwrap();
    let process = sandbox
        .executor()
        .spawn(&Spawn::program("/bin/echo").arg("hi"))
        .await
        .unwrap();
    let mut output = Vec::new();
    let ending = process
        .ended(|_stream, data| output.extend_from_slice(&data))
        .await
        .unwrap();
    let lease_cgroup = host.config.cgroup_root.join("lease-1");
    let joined = fs::read_to_string(lease_cgroup.join(SANDBOX_LEAF).join(CGROUP_PROCS)).unwrap();
    let io_limit = fs::read_to_string(lease_cgroup.join("io.max")).unwrap();
    let destroyed = sandbox.destroy().await;
    server.abort();

    assert_eq!(
        (ending, output.as_slice()),
        (Ending::Exited(0), b"hi\n".as_slice())
    );
    assert_eq!(
        joined, "0",
        "bubblewrap joined the lease's sandbox leaf before it ran"
    );
    assert!(io_limit.contains("wbps="), "{io_limit}");
    // Nothing was mounted, so the unmount is refused: the image stays for the
    // boot sweep rather than being unlinked under an attached loop device.
    destroyed.unwrap_err();
    assert!(host.lease_dir("lease-1").join(IMAGE).exists());
}

/// The fake host's mount makes no loop device, so there is none to switch to
/// direct I/O: the lease still starts, and is named as one whose disk the
/// host caches a second time.
#[tokio::test]
async fn test_a_disk_without_a_loop_device_is_logged_buffered_under_its_lease() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let capture = Capture::install();

    let sandbox = host.engine().prepare(request("lease-13")).await.unwrap();
    let _left = sandbox.destroy().await;
    server.abort();

    assert_eq!(
        capture.only(EVENT_DISK_BUFFERED).field("lease_id"),
        Some("lease-13")
    );
}

// bubblewrap and its host-side monitor keep the environment they were started
// with, readable through `/proc/<pid>/environ`; the runner's own carries its
// credential, so the launcher must start with none.
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "asks only whether a variable is present, as raw OS bytes; afd_core::env reads String values and would call a non-Unicode one unset"
)]
async fn test_bubblewrap_starts_with_an_empty_environment() {
    assert!(
        ENV_PROBE_VARIABLES
            .iter()
            .any(|name| std::env::var_os(name).is_some()),
        "the test process sets a variable the probe can see, or this proves nothing"
    );
    let host = FakeHost::new(ENV_PROBE);

    let refused = host
        .engine()
        .prepare(request("lease-env"))
        .await
        .unwrap_err();

    let quoted = refused.to_string();
    assert!(quoted.contains(ENV_PROBE_REPORT), "{quoted}");
    assert!(!quoted.contains(ENV_PROBE_INHERITED), "{quoted}");
}

#[tokio::test]
async fn test_a_sandbox_that_exits_refuses_the_lease_with_its_reason() {
    let host = FakeHost::new(FAILER);
    let capture = Capture::install();

    let refused = host.engine().prepare(request("lease-2")).await.unwrap_err();

    let quoted = refused.to_string();
    assert!(quoted.contains(FAILER_REASON), "{quoted}");
    assert!(
        quoted.contains("\n30\n") && !quoted.contains("\n10\n"),
        "only the last lines: {quoted}"
    );
    let failed = capture.only("sandbox_prepare_failed");
    assert_eq!(failed.field("lease_id"), Some("lease-2"));
    assert!(failed.field("error_code").is_some());
    assert_eq!(
        capture.only("sandbox_teardown_failed").field("lease_id"),
        Some("lease-2")
    );
}

#[tokio::test]
async fn test_a_sandbox_that_never_answers_refuses_the_lease() {
    let mut host = FakeHost::new(SLEEPER);
    host.config.ready_timeout = Duration::from_millis(20);

    let refused = host.engine().prepare(request("lease-3")).await.unwrap_err();

    assert!(refused.to_string().contains("did not answer"), "{refused}");
    assert!(
        std::error::Error::source(&refused).is_some(),
        "why the last attempt failed is kept"
    );
}

#[tokio::test]
async fn test_a_lease_never_inherits_a_directory_already_there() {
    let host = FakeHost::new(SLEEPER);
    let engine = host.engine();
    fs::create_dir_all(host.lease_dir("lease-4").join("workspace")).unwrap();

    engine.prepare(request("lease-4")).await.unwrap_err();

    assert!(
        host.lease_dir("lease-4").join("workspace").exists(),
        "left for the boot sweep"
    );
}

#[tokio::test]
async fn test_a_lease_identifier_that_escapes_is_refused_before_anything_is_built() {
    let host = FakeHost::new(SLEEPER);

    let refused = host
        .engine()
        .prepare(request("../lease-5"))
        .await
        .unwrap_err();

    assert!(
        refused.to_string().contains("single path component"),
        "{refused}"
    );
    assert!(!host.dir.path().join("lease-5").exists());
}

#[tokio::test]
async fn test_a_workspace_disk_that_cannot_be_made_refuses_the_lease_and_leaves_nothing() {
    let mut host = FakeHost::new(SLEEPER);
    host.config.tools.mke2fs = PathBuf::from(FALSE);

    let refused = host.engine().prepare(request("lease-6")).await.unwrap_err();

    assert!(refused.to_string().contains("mke2fs exited"), "{refused}");
    assert!(!host.lease_dir("lease-6").exists());
}

#[tokio::test]
async fn test_a_launcher_that_cannot_start_refuses_the_lease() {
    let mut host = FakeHost::new(SLEEPER);
    host.config.tools.bwrap = host.dir.path().join("absent-bwrap");

    host.engine().prepare(request("lease-7")).await.unwrap_err();
}

#[tokio::test]
async fn test_a_cgroup_that_cannot_be_made_refuses_the_lease() {
    let host = FakeHost::new(SLEEPER);
    fs::create_dir(host.config.cgroup_root.join("lease-9")).unwrap();

    let refused = host.engine().prepare(request("lease-9")).await.unwrap_err();

    assert_eq!(
        refused.missing_mechanism(),
        None,
        "one lease failed, not the host"
    );
}

#[test]
fn test_a_host_without_landlock_refuses_every_lease() {
    let mut host = FakeHost::new(SLEEPER);
    host.probe.landlock = false;

    let refused = BubblewrapEngine::new(host.config.clone(), &host.probe).unwrap_err();

    assert_eq!(refused.missing_mechanism(), Some("landlock"));
}

#[tokio::test]
async fn test_a_sandbox_that_dies_reports_it_is_no_longer_running() {
    let host = FakeHost::new(BRIEF);
    let server = serve_leases(host.config.state_dir.clone());
    let mut sandbox = host.engine().prepare(request("lease-10")).await.unwrap();

    let alive = sandbox.is_running();
    while sandbox.is_running() {
        tokio::time::sleep(POLL).await;
    }
    let _left = sandbox.destroy().await;
    server.abort();

    assert!(alive, "running while bubblewrap is");
}

#[tokio::test]
async fn test_warm_slots_hand_out_a_bubblewrap_sandbox_started_ahead() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let capture = Capture::install();
    let slots = WarmSlots::start(Arc::new(host.engine()), 1, LIMITS);
    while !host
        .leases()
        .iter()
        .any(|lease| host.lease_dir(lease).join(SANDBOXED).exists())
    {
        tokio::time::sleep(POLL).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let warm = slots.prepare(request("lease-8")).await.unwrap();
    let _left = warm.destroy().await;
    slots.shutdown().await;
    server.abort();

    let started = capture
        .events()
        .into_iter()
        .find(|event| event.field("event") == Some("sandbox_start_completed"));
    assert_eq!(
        started
            .and_then(|event| event.field("start").map(str::to_owned))
            .as_deref(),
        Some("warm")
    );
    assert!(
        host.leases()
            .iter()
            .all(|lease| lease.starts_with("warm-") && lease.len() > 30),
        "slots are named like leases: {:?}",
        host.leases()
    );
}

#[tokio::test]
async fn test_a_warm_slot_that_died_is_discarded_and_the_lease_starts_cold() {
    let host = FakeHost::new(BRIEF);
    let server = serve_leases(host.config.state_dir.clone());
    let capture = Capture::install();
    let slots = WarmSlots::start(Arc::new(host.engine()), 1, LIMITS);
    tokio::time::sleep(Duration::from_millis(600)).await;

    let cold = slots.prepare(request("lease-11")).await.unwrap();
    let _left = cold.destroy().await;
    slots.shutdown().await;
    server.abort();

    assert!(
        capture
            .only("sandbox_warm_slot_died")
            .field("error_code")
            .is_some()
    );
}

/// A root runner starts bubblewrap as the sandbox's own host user.
/// Unprivileged, the one user this test may name is its own, so it proves
/// that a runner naming a user still starts its sandbox; the kernel lane
/// proves who owns what the sandbox writes.
#[tokio::test]
async fn test_a_root_runner_starts_the_sandbox_as_its_own_user() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let mut engine = host.engine();
    let me = (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    );
    engine.run_as = Some(me);
    engine.owner = me;

    let mut sandbox = engine.prepare(request("lease-12")).await.unwrap();
    let running = sandbox.is_running();
    let _left = sandbox.destroy().await;
    server.abort();

    assert!(running, "started as the named user");
}

/// A kill proves the sandbox empty only once its cgroup reads `populated 0`:
/// with no events to read it is unsettled, and once they say so it settles.
#[tokio::test]
async fn test_a_kill_settles_only_once_the_cgroup_reads_empty() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let sandbox = host.engine().prepare(request("lease-13")).await.unwrap();

    let unread = sandbox.kill().await;
    fs::write(
        host.config
            .cgroup_root
            .join("lease-13")
            .join("cgroup.events"),
        "populated 0\n",
    )
    .unwrap();
    let read = sandbox.kill().await;
    let _left = sandbox.destroy().await;
    server.abort();

    assert!(matches!(unread, Killed::Unsettled(_)), "{unread:?}");
    assert!(matches!(read, Killed::Settled), "{read:?}");
}

/// Parts that never made a cgroup hold no process to kill and so prove
/// nothing: their kill is unsettled.
#[tokio::test]
async fn test_parts_with_no_cgroup_kill_unsettled() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("lease-14");
    fs::create_dir(&dir).unwrap();
    let parts = Parts::new("lease-14", dir);

    let killed = parts.kill().await;

    assert!(matches!(killed, Killed::Unsettled(_)), "{killed:?}");
}

/// A sandbox whose entry dropped its end of the link while it runs is
/// refused as not ready, once it had a moment to exit and did not.
#[tokio::test]
async fn test_a_sandbox_that_drops_its_link_is_refused_as_not_ready() {
    let host = FakeHost::new(CLOSER);

    let refused = host
        .engine()
        .prepare(request("lease-15"))
        .await
        .unwrap_err();

    let said = refused.to_string();
    assert!(said.contains("did not answer within"), "{said}");
}

/// What bubblewrap's child does before its exec: it writes itself into the
/// sandbox leaf through the descriptor the runner opened.
#[test]
fn test_the_child_joins_the_sandbox_leaf_before_its_exec() {
    let leaf = tempfile::tempdir().unwrap();
    let procs = leaf.path().join("procs");
    let join = fs::File::create(&procs).unwrap();
    let tenant = TenantFiles::open(
        &leaf.path().join("cgroup.procs"),
        &leaf.path().join("memory.events"),
    )
    .unwrap();
    let (handover, _runner) = Handover::with_link(tenant).unwrap();

    in_child(&join, &handover).unwrap();

    assert_eq!(fs::read_to_string(procs).unwrap(), "0");
}
