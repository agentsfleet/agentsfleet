//! The degraded stop: one handler registered instead of two.
//!
//! `shutdown` watches SIGTERM and SIGINT, and declines to refuse boot when the
//! first of those will not register — one handler still stops the process, and
//! a daemon that would not start over it would be strictly worse. That branch
//! is the kind of code nobody exercises until the day it matters, so it is
//! exercised here.
//!
//! # How a registration is made to fail
//!
//! Tokio refuses the signals a process cannot catch, so
//! `SignalKind::from_raw(SIGKILL)` fails through the same public call SIGTERM
//! goes through. That is why [`agentsfleetd::signal::stop_on_kind`] takes the
//! kind as an argument: nothing else about a real SIGTERM registration can be
//! made to fail on demand.
//!
//! # Why raising a signal at the test process is safe here
//!
//! SIGINT's default disposition is to terminate. The guard registered on the
//! first line replaces that disposition for the whole process BEFORE anything
//! is raised, so by the time `kill(1)` runs there is a handler to catch it.
//! Each integration test file is its own process, so nothing else is affected.
#![cfg(all(unix, feature = "test-util"))]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::process::Command;
use std::time::Duration;

use tokio::signal::unix::{SignalKind, signal};

/// The signal number no process may catch, which is the point of using it.
const SIGKILL: i32 = 9;

/// How long the fallback is given to observe an interrupt.
const PATIENCE: Duration = Duration::from_secs(10);

/// How often the interrupt is re-sent while waiting.
const RETRY: Duration = Duration::from_millis(50);

/// Sends SIGINT to this process.
///
/// Through `kill(1)` rather than `libc::raise`: this workspace links no libc
/// and forbids unsafe code, and one fork per retry is nothing against a test
/// that is waiting on a signal anyway.
fn interrupt_self() {
    let sent = Command::new("kill")
        .args(["-INT", &std::process::id().to_string()])
        .status()
        .expect("kill(1) runs");
    assert!(sent.success(), "SIGINT was delivered to this process");
}

/// A terminate kind that will not register leaves the interrupt handler working.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_kind_that_will_not_register_falls_back_to_interrupt() {
    let _guard = signal(SignalKind::interrupt()).expect("SIGINT registers, replacing its default");

    assert!(
        signal(SignalKind::from_raw(SIGKILL)).is_err(),
        "the premise: SIGKILL is a registration that fails, which SIGTERM never does"
    );

    let stopping = tokio::spawn(agentsfleetd::signal::stop_on_kind(SignalKind::from_raw(
        SIGKILL,
    )));

    // Re-sent rather than sent once: an interrupt that lands before the task
    // has polled its `ctrl_c` future is not delivered to it, and retrying is
    // cheaper and steadier than sleeping long enough to be sure it has.
    let observed = tokio::time::timeout(PATIENCE, async {
        while !stopping.is_finished() {
            interrupt_self();
            tokio::time::sleep(RETRY).await;
        }
    })
    .await;

    assert!(
        observed.is_ok(),
        "with SIGTERM unregistered, SIGINT alone must still stop the daemon"
    );
    stopping.await.expect("the fallback path did not panic");
}
