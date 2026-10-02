//! A descendant that leaves its process's group and session keeps the output
//! open; the process is still reported ended, on time, exactly once.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::{Duration, Instant};

use afr_executor::{Ending, Executor as _, Spawn};
use rustix::process::{Pid, Signal, kill_process};

use crate::support::{finish, start};

/// The leader forks and exits at once; the child starts a session of its own,
/// says its pid, and sleeps holding the output. `setsid sleep 600 &` without
/// the `setsid` binary, which macOS does not ship.
const ORPHAN: &str = r#"use POSIX (); if (fork) { exit 0 } POSIX::setsid(); $| = 1; print "$$\n"; exec "sleep", "600""#;

/// Long enough for the drain grace and a loaded machine, far short of the
/// orphan's sleep.
const PROMPTLY: Duration = Duration::from_secs(10);

/// Runs the orphan maker and answers how it ended, how long that took, and
/// the orphan's pid from whatever output arrived.
async fn orphaned(spawn: Spawn) -> (Vec<(Ending, u64)>, Duration, Option<Pid>) {
    let harness = start().await;
    let started = Instant::now();

    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    let said = [finished.stdout, finished.terminal].concat();
    let pid = String::from_utf8_lossy(&said)
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw);
    (finished.endings, started.elapsed(), pid)
}

/// Ends the orphan the test left behind.
fn reap(pid: Option<Pid>) {
    if let Some(pid) = pid {
        let _gone_already = kill_process(pid, Signal::KILL);
    }
}

#[tokio::test]
async fn a_descendant_in_its_own_session_does_not_hold_the_end_of_a_pipe_process() {
    let (endings, took, pid) = orphaned(Spawn::program("perl").args(["-e", ORPHAN])).await;
    reap(pid);

    assert_eq!(endings, [(Ending::Exited(0), 0)]);
    assert!(took < PROMPTLY, "reported ended after {took:?}");
    assert!(
        pid.is_some(),
        "the orphan said its pid before the drain ended"
    );
}

#[tokio::test]
async fn a_descendant_in_its_own_session_does_not_hold_the_end_of_a_terminal_process() {
    let spawn = Spawn::program("perl").args(["-e", ORPHAN]).terminal();
    let (endings, took, pid) = orphaned(spawn).await;
    reap(pid);

    assert_eq!(endings, [(Ending::Exited(0), 0)]);
    assert!(took < PROMPTLY, "reported ended after {took:?}");
}

#[tokio::test]
async fn a_timeout_still_ends_a_process_whose_descendant_left_the_group() {
    let harness = start().await;
    // The orphan holds the output while the leader outlives its timeout.
    let spawn = Spawn::program("perl")
        .args(["-e", ORPHAN_AND_WAIT])
        .timeout(Duration::from_millis(200));
    let started = Instant::now();

    let finished = finish(harness.client.spawn(spawn).await.unwrap()).await;

    let pid = String::from_utf8_lossy(&finished.stdout)
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw);
    reap(pid);
    assert_eq!(finished.endings, [(Ending::TimedOut, 0)]);
    assert!(started.elapsed() < PROMPTLY, "{:?}", started.elapsed());
}

/// [`ORPHAN`], but the leader stays until it is killed.
const ORPHAN_AND_WAIT: &str = r#"use POSIX (); if (my $child = fork) { sleep 600; exit 0 } POSIX::setsid(); $| = 1; print "$$\n"; exec "sleep", "600""#;
