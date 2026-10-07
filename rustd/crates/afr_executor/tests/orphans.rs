//! A descendant that leaves its process's group and session keeps the output
//! open; the process is still reported ended, on time, exactly once.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::{Duration, Instant};

use afr_executor::{Ending, Executor as _, Spawn};
use rustix::process::{Pid, Signal, kill_process, test_kill_process};

use crate::support::{Finished, finish, start};

/// The leader forks and exits; the child starts a session of its own, says
/// its pid, and sleeps holding the output. `setsid sleep 600 &` without the
/// `setsid` binary, which macOS does not ship.
///
/// The leader waits on a pipe until the child has left its group. Exiting at
/// once raced the executor, which ends a leader's group the moment the leader
/// exits: under load that kill landed before `setsid`, and the child died
/// without ever becoming the orphan this file is about.
const ORPHAN: &str = r#"use POSIX (); pipe(my $r, my $w); if (fork) { close $w; <$r>; exit 0 } close $r; POSIX::setsid(); $| = 1; print "$$\n"; close $w; exec "sleep", "600""#;

/// Long enough for the drain grace and a loaded machine, far short of the
/// orphan's sleep.
const PROMPTLY: Duration = Duration::from_secs(10);

/// Runs the orphan maker and answers how it ended, how long that took, and
/// the orphan's pid from whatever output arrived.
async fn orphaned(spawn: Spawn) -> (Finished, Duration, Option<Pid>) {
    let harness = start().await;
    let started = Instant::now();

    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    let said = [finished.stdout.as_slice(), finished.terminal.as_slice()].concat();
    let pid = String::from_utf8_lossy(&said)
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw);
    (finished, started.elapsed(), pid)
}

/// Ends the orphan the test left behind.
fn reap(pid: Option<Pid>) {
    if let Some(pid) = pid {
        let _gone_already = kill_process(pid, Signal::KILL);
    }
}

#[tokio::test]
async fn a_descendant_in_its_own_session_does_not_hold_the_end_of_a_pipe_process() {
    let (finished, took, pid) = orphaned(Spawn::program("perl").args(["-e", ORPHAN])).await;
    reap(pid);

    assert_eq!(finished.endings, [Ending::Exited(0)]);
    assert!(took < PROMPTLY, "reported ended after {took:?}");
    assert!(
        pid.is_some(),
        "the orphan said its pid before the drain ended"
    );
    assert!(
        finished.abandoned,
        "the orphan held the output open past the grace, and the ending says so"
    );
}

#[tokio::test]
async fn a_descendant_in_its_own_session_does_not_hold_the_end_of_a_terminal_process() {
    let spawn = Spawn::program("perl").args(["-e", ORPHAN]).terminal();
    let (finished, took, pid) = orphaned(spawn).await;
    reap(pid);

    assert_eq!(finished.endings, [Ending::Exited(0)]);
    // Whether the terminal's output closes with its leader or stays held by
    // the orphan is the platform's call, so nothing is asserted of it here.
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

    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    let pid = String::from_utf8_lossy(&finished.stdout)
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw);
    reap(pid);
    assert_eq!(finished.endings, [Ending::TimedOut]);
    assert!(started.elapsed() < PROMPTLY, "{:?}", started.elapsed());
}

/// [`ORPHAN`], but the leader stays until it is killed.
const ORPHAN_AND_WAIT: &str = r#"use POSIX (); if (my $child = fork) { sleep 600; exit 0 } POSIX::setsid(); $| = 1; print "$$\n"; exec "sleep", "600""#;

/// A job `sh` puts in the background without job control stays in the
/// command's process group; its output goes elsewhere, so only the group's
/// kill can end it. It prints the job's pid.
const BACKGROUNDED: &str = "sleep 600 >/dev/null 2>&1 & echo $!";
/// How often a job that should be gone is looked for.
const LOOK_AGAIN: Duration = Duration::from_millis(20);

/// Whether `pid` names no process before `limit` passes. A killed job is
/// reaped by whoever adopted it, so it may outlive its kill by a moment.
async fn gone_within(pid: Pid, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while test_kill_process(pid).is_ok() {
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(LOOK_AGAIN).await;
    }
    true
}

/// A backgrounded job left in the group ends with its command: once the
/// leader ends, the executor kills its group. A descendant in a session of
/// its own outlives that kill, which the tests above prove by the output it
/// holds open past the drain's grace.
#[tokio::test]
async fn a_backgrounded_job_in_the_group_ends_with_its_command() {
    let harness = start().await;
    let spawn = Spawn::program("sh").args(["-c", BACKGROUNDED]);

    let finished = finish(harness.client.spawn(&spawn).await.unwrap()).await;

    let pid = String::from_utf8_lossy(&finished.stdout)
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw);
    let gone = match pid {
        Some(pid) => gone_within(pid, PROMPTLY).await,
        None => false,
    };
    reap(pid);
    assert_eq!(finished.endings, [Ending::Exited(0)]);
    assert!(pid.is_some(), "the shell said its job's pid");
    assert!(gone, "the job ended with its command's group");
}
