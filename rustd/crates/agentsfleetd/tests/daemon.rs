//! Dimensions 7.2 and 7.5 — the boot window, and an inventory that cannot drift.
//!
//! Every wait is a handshake. `test_boot_window_sigterm` in particular must not
//! be a race: the signal is made to have ALREADY arrived before the server is
//! polled, which is the boot-window condition stated as a fact rather than
//! approximated with a sleep and hoped for.
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agentsfleetd::daemon::{Daemon, StopCause};
use agentsfleetd::inventory::{ANALYTICS_FLUSH, BACKGROUND_TASKS, HUB_PUMP, OTLP_EXPORT};
use agentsfleetd::supervisor::Supervisor;

use self::support::install_subscriber;

/// Spawns a supervised task that records having observed its cancellation.
///
/// Returns the flag, so a test can assert the task was CANCELLED rather than
/// merely joined — the difference between a teardown that ran and one that was
/// skipped on a path nobody exercised.
fn spawn_observer(supervisor: &mut Supervisor, name: &'static str) -> Arc<AtomicBool> {
    let observed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&observed);
    supervisor.spawn(name, move |token| async move {
        token.cancelled().await;
        flag.store(true, Ordering::SeqCst);
    });
    observed
}

/// Dimension 7.2 — a signal during boot stops the daemon, and says it was one.
///
/// The Zig daemon needs two flags for this because its watcher is a separate
/// thread polling every 100ms, so "the signal arrived" and "the server
/// stopped" are events that race. Here the signal future is already resolved
/// when `run` polls it, so there is nothing to race: the daemon must report
/// `Signalled` even though the server was equally ready to finish.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_boot_window_sigterm() {
    install_subscriber();

    let was_polled = Arc::new(AtomicBool::new(false));
    let mut supervisor = Supervisor::new();
    let observed = spawn_observer(&mut supervisor, "background");

    // Ready to finish the instant it is polled, so if the signal did not
    // already own this race the server would win it.
    let server = {
        let was_polled = Arc::clone(&was_polled);
        async move { was_polled.store(true, Ordering::SeqCst) }
    };

    let outcome = Daemon::new(supervisor)
        .run(
            server,
            // Already resolved: the signal arrived before serving began.
            std::future::ready(()),
        )
        .await;

    assert_eq!(
        outcome.cause,
        StopCause::Signalled,
        "a signal that arrived during boot is a signal, not a server that fell over"
    );
    assert!(
        outcome.shutdown.is_clean(),
        "the background fleet is still torn down: {:?}",
        outcome.shutdown
    );
    assert!(
        observed.load(Ordering::SeqCst),
        "the supervised task was cancelled, not abandoned when the signal won"
    );
    assert!(
        !was_polled.load(Ordering::SeqCst),
        "the signal had already arrived, so the server must never have been polled — \
         that window is exactly what the two Zig flags exist to protect"
    );
    assert!(
        outcome.is_clean(),
        "a signalled stop with a clean join is clean"
    );
}

/// A server that ends on its own is reported as such, and still tears down.
///
/// The case `serve.zig` does not model: a lost bind or an accept loop that
/// returned. A daemon that only waits for a signal hangs here, and a hung
/// process explains nothing on its way out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_server_that_stops_on_its_own_is_named_and_torn_down() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    let observed = spawn_observer(&mut supervisor, "background");

    let outcome = Daemon::new(supervisor)
        .run(std::future::ready(()), std::future::pending())
        .await;

    assert_eq!(
        outcome.cause,
        StopCause::ServerStopped,
        "nothing signalled, so the server ending is the reason"
    );
    assert!(
        observed.load(Ordering::SeqCst),
        "teardown runs on this path too — that is what the outer run() is for"
    );
    assert!(
        !outcome.is_clean(),
        "a server that fell over is not a clean run, however clean the join was"
    );
}

/// Teardown runs even when a supervised task refuses to stop.
///
/// Time is paused, so the join timeout costs no wall clock. The assertion is
/// that the outcome REPORTS the straggler rather than the run pretending it
/// went well.
#[tokio::test(start_paused = true)]
async fn test_an_unclean_teardown_is_reported_not_swallowed() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    supervisor.spawn("ignores_its_token", |_token| std::future::pending());

    let outcome = Daemon::new(supervisor)
        .run(std::future::pending(), std::future::ready(()))
        .await;

    assert_eq!(outcome.cause, StopCause::Signalled);
    assert_eq!(
        outcome.shutdown.abandoned,
        vec!["ignores_its_token"],
        "the straggler is named"
    );
    assert!(
        !outcome.is_clean(),
        "a signalled stop that abandoned a task is not clean"
    );
}

/// The daemon supervises exactly the background tasks it claims to.
///
/// Asserted by NAME rather than by count: a task quietly added to or dropped
/// from boot is exactly the drift a count would wave through.
///
/// What each Zig thread became is in `docs/architecture/concurrency.md`. It is
/// not duplicated here, and was briefly: a porting ledger is prose about a
/// migration, and prose in a table of tuples is harder to read, harder to
/// change, and no more true than the document.
#[test]
fn test_the_daemon_supervises_what_it_claims() {
    assert_eq!(
        BACKGROUND_TASKS,
        &[HUB_PUMP, OTLP_EXPORT, ANALYTICS_FLUSH],
        "this build supervises the pub/sub pump, the span exporter and the \
         analytics flush, and nothing else"
    );
}

/// A daemon reports the tasks it holds before it runs them.
#[tokio::test]
async fn test_a_daemon_reports_its_inventory_before_running() {
    let mut supervisor = Supervisor::new();
    supervisor.spawn(HUB_PUMP, |token| async move { token.cancelled().await });
    supervisor.spawn(OTLP_EXPORT, |token| async move { token.cancelled().await });
    supervisor.spawn(ANALYTICS_FLUSH, |token| async move {
        token.cancelled().await;
    });

    let daemon = Daemon::new(supervisor);
    assert_eq!(
        daemon.inventory(),
        BACKGROUND_TASKS,
        "what boot spawned must equal what the inventory says it would"
    );

    let outcome = daemon
        .run(std::future::pending(), std::future::ready(()))
        .await;
    assert!(outcome.is_clean());
}
