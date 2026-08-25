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
use agentsfleetd::inventory::{
    Disposition, HUB_PUMP, OTLP_EXPORT, THREAD_MAP, deferred_rows, supervised_names,
};
use agentsfleetd::supervisor::Supervisor;

use self::support::install_subscriber;

/// Rows in the `concurrency.md` `agentsfleetd` thread map.
///
/// Pinned, so a row deleted from the table fails here rather than quietly
/// shrinking what "complete inventory" means.
const THREAD_MAP_ROWS: usize = 11;

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

/// Dimension 7.5 — every thread-map row has a disposition, and none is silent.
#[test]
fn test_task_inventory_covers_every_thread_map_row() {
    assert_eq!(
        THREAD_MAP.len(),
        THREAD_MAP_ROWS,
        "the inventory must carry one row per concurrency.md thread-map row"
    );

    for row in THREAD_MAP {
        assert!(!row.zig.is_empty(), "a row with no name names nothing");
        match row.disposition {
            Disposition::Supervised(name) => {
                assert!(!name.is_empty(), "{} is supervised as nothing", row.zig);
            }
            Disposition::Retired(why) => {
                assert!(
                    why.len() > row.zig.len(),
                    "{}: a retirement needs a reason longer than the row name",
                    row.zig
                );
            }
            Disposition::Deferred(milestone) => {
                assert!(
                    milestone.starts_with('M') && milestone.len() >= 4,
                    "{} is deferred to {milestone:?}, which is not a milestone id",
                    row.zig
                );
            }
        }
    }
}

/// What this build supervises, and what it still owes.
///
/// Asserted by NAME rather than by count: a row quietly changing disposition
/// from deferred to supervised — or the reverse — is exactly the drift a count
/// would wave through.
#[test]
fn test_the_inventory_names_what_is_supervised_and_what_is_owed() {
    assert_eq!(
        supervised_names(),
        vec![HUB_PUMP, OTLP_EXPORT],
        "this build supervises the pub/sub pump and the span exporter, and nothing else"
    );

    let deferred = deferred_rows();
    assert_eq!(
        deferred.len(),
        THREAD_MAP_ROWS - supervised_names().len() - 2,
        "eleven rows, two supervised, two retired by design — the rest are owed"
    );
    for (row, milestone) in deferred {
        assert!(
            !row.is_empty() && !milestone.is_empty(),
            "a deferral names both the work and who owes it"
        );
    }
}

/// A daemon reports the tasks it holds before it runs them.
#[tokio::test]
async fn test_a_daemon_reports_its_inventory_before_running() {
    let mut supervisor = Supervisor::new();
    supervisor.spawn(HUB_PUMP, |token| async move { token.cancelled().await });
    supervisor.spawn(OTLP_EXPORT, |token| async move { token.cancelled().await });

    let daemon = Daemon::new(supervisor);
    assert_eq!(
        daemon.inventory(),
        supervised_names(),
        "what boot spawned must equal what the inventory says it would"
    );

    let outcome = daemon
        .run(std::future::pending(), std::future::ready(()))
        .await;
    assert!(outcome.is_clean());
}
