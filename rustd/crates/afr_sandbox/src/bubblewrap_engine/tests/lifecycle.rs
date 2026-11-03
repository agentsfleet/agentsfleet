//! A sandbox's life past its start, on the fake host: a kill settles only
//! once its cgroup reads empty, a lost link is refused as not ready, a wait
//! cut short keeps the error stream, and bubblewrap's child joins its leaf
//! and lets its descriptors through before its exec.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;
use std::os::fd::RawFd;
use std::os::unix::fs::PermissionsExt as _;
use std::time::{Duration, Instant};

use super::super::parts::{Parts, in_child};
use super::support::{CLOSER, FakeHost, SLEEPER, request, serve_leases};
use crate::engine::{Engine as _, Killed};
use crate::tenant::{Handover, TenantFiles};

/// Well under the fake host's 5 s ready wait: a lost link is refused once the
/// sandbox has had its moment to exit, never at the ready timeout.
const LOST_LINK_WITHIN: Duration = Duration::from_secs(3);
/// How long the first wait for an exit lasts: past the process, short of
/// the error stream it left open.
const CUT_SHORT: Duration = Duration::from_millis(50);
/// How long the second wait may last: far past the straggler's 0.3 s, so only
/// a wait that would never end runs into it.
const PATIENCE: Duration = Duration::from_secs(5);
/// `O_CLOEXEC`, as `/proc/self/fdinfo` prints a descriptor's flags.
const O_CLOEXEC: u32 = 0o2_000_000;

/// Whether this process's descriptor `fd` closes on exec.
fn closes_on_exec(fd: RawFd) -> bool {
    let info = fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).unwrap();
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:"))
        .unwrap();
    u32::from_str_radix(flags.trim(), 8).unwrap() & O_CLOEXEC != 0
}

/// The tenant leaf's files in a plain directory, with the executor's pair.
fn handover(leaf: &std::path::Path) -> Handover {
    let tenant =
        TenantFiles::open(&leaf.join("cgroup.procs"), &leaf.join("memory.events")).unwrap();
    Handover::with_link(tenant).unwrap().0
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
/// refused as not ready once it had a moment to exit and did not, well
/// before the ready timeout.
#[tokio::test]
async fn test_a_sandbox_that_drops_its_link_is_refused_as_not_ready() {
    let host = FakeHost::new(CLOSER);
    let started = Instant::now();

    let refused = host
        .engine()
        .prepare(request("lease-15"))
        .await
        .unwrap_err();

    let took = started.elapsed();
    let said = refused.to_string();
    assert!(said.contains("did not answer within"), "{said}");
    assert!(took < LOST_LINK_WITHIN, "refused after {took:?}");
}

/// A process can exit while something it started still holds its error
/// stream. A wait for its exit cut short mid-drain leaves the stream for the
/// next wait, which reads the whole reason.
#[tokio::test]
async fn test_a_wait_cut_short_keeps_the_error_stream() {
    let base = tempfile::tempdir().unwrap();
    let lease = base.path().join("lease-16");
    fs::create_dir(&lease).unwrap();
    let script = base.path().join("straggler");
    fs::write(
        &script,
        "#!/bin/sh\n(sleep 0.3; echo held-open-reason >&2) &\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let mut parts = Parts::new("lease-16", lease.clone());
    let handed = handover(&lease);
    parts
        .spawn(&script, Vec::new(), &lease.join("procs"), handed, None)
        .unwrap();

    let cut_short = tokio::time::timeout(CUT_SHORT, parts.exited()).await;
    // Bounded: with no child left to wait on, `exited` never resolves.
    let exited = tokio::time::timeout(PATIENCE, parts.exited()).await;

    assert!(
        cut_short.is_err(),
        "the error stream outlasted the first wait"
    );
    let said = exited.map(|error| error.to_string());
    assert!(
        said.is_ok(),
        "the second wait for bubblewrap's exit outlasted {PATIENCE:?}"
    );
    let said = said.unwrap();
    assert!(said.contains("held-open-reason"), "{said}");
}

/// What bubblewrap's child does before its exec: it writes itself into the
/// sandbox leaf through the descriptor the runner opened, and lets all three
/// handed-over descriptors survive the exec.
#[test]
fn test_the_child_joins_its_leaf_and_lets_its_descriptors_through() {
    let leaf = tempfile::tempdir().unwrap();
    let procs = leaf.path().join("procs");
    let join = fs::File::create(&procs).unwrap();
    let handed = handover(leaf.path());
    let named = handed.descriptors();
    let numbers = [
        named.tenant.tenant_procs,
        named.tenant.tenant_events,
        named.executor,
    ];
    assert!(numbers.into_iter().all(closes_on_exec), "closed before");

    in_child(&join, &handed).unwrap();

    assert_eq!(fs::read_to_string(procs).unwrap(), "0");
    assert!(
        !numbers.into_iter().any(closes_on_exec),
        "kept across the exec"
    );
}
