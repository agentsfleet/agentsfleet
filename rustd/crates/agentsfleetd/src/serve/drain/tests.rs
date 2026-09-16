//! What one drain does: the count it reports, the bound it honours, and the
//! two signals it waits on.
//!
//! Split out of `drain.rs` rather than living inline: the file was at its
//! length cap with no room for the cases the accept-loop handshake needs.

#![expect(
    clippy::expect_used,
    reason = "a joined test task's panic should surface here, not be swallowed"
)]

use super::*;

/// Long enough that a scheduler hiccup cannot fail a test that is meant to
/// prove a clean drain, short enough that the timeout tests stay quick.
const GENEROUS: Duration = Duration::from_secs(5);
/// A subscriber for the length of one test, so the `tracing` macros on the
/// drain's two diagnostic paths EVALUATE their fields. Without one the macros
/// short-circuit and the lines an operator reads a stuck deployment by are
/// never executed at all.
fn recording() -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    )
}
const IMPATIENT: Duration = Duration::from_millis(50);
/// What a simulated connection or accept loop spends before it finishes. Long
/// enough that `settle` is genuinely waiting on it rather than racing it.
const A_MOMENT: Duration = Duration::from_millis(20);

#[tokio::test]
async fn an_empty_server_drains_at_once_and_stops_accepting() {
    let drain = Drain::new();
    assert!(!drain.accepting().is_cancelled());
    let settled = drain.settle(GENEROUS).await;
    assert_eq!(
        settled,
        Settled {
            in_flight_at_close: 0,
            abandoned: 0
        }
    );
    assert!(settled.is_clean());
    // The accept loop's stop is the drain's first act, not a separate step
    // a caller could forget.
    assert!(drain.accepting().is_cancelled());
}

#[tokio::test]
async fn an_in_flight_request_finishes_before_the_drain_returns() {
    let _logs = recording();
    let drain = Drain::new();
    let guard = drain.enter();
    assert_eq!(drain.in_flight(), 1);

    let worker = tokio::spawn(async move {
        tokio::time::sleep(A_MOMENT).await;
        drop(guard);
    });

    let settled = drain.settle(GENEROUS).await;
    // The point of the whole module: the count was 1 when accepting stopped
    // and 0 when the drain returned, so the request completed rather than
    // being cut.
    assert_eq!(settled.in_flight_at_close, 1);
    assert_eq!(settled.abandoned, 0);
    assert!(settled.is_clean());
    worker.await.expect("the connection task finished");
}

#[tokio::test]
async fn a_request_that_outlasts_the_bound_is_reported_not_waited_for() {
    let _logs = recording();
    let drain = Drain::new();
    let held = drain.enter();

    let settled = drain.settle(IMPATIENT).await;
    // Not a failure — a finished drain with something left, named so an
    // operator reads a number instead of watching a process that will not die.
    assert_eq!(settled.in_flight_at_close, 1);
    assert_eq!(settled.abandoned, 1);
    assert!(!settled.is_clean());
    drop(held);
}

#[tokio::test]
async fn every_connection_is_counted_and_released() {
    let drain = Drain::new();
    let guards: Vec<Guard> = (0..8).map(|_| drain.enter()).collect();
    assert_eq!(drain.in_flight(), 8);
    drop(guards);
    assert_eq!(drain.in_flight(), 0);
    assert!(drain.settle(GENEROUS).await.is_clean());
}

/// The race the `enable()` call in `idle` exists for: a guard dropped in the
/// window between the count check and the registration must still wake the
/// waiter, or the drain waits out its entire bound over an empty server.
#[tokio::test]
async fn a_guard_dropped_while_the_drain_is_arming_still_wakes_it() {
    for _ in 0..64 {
        let drain = Drain::new();
        let guard = drain.enter();
        let releaser = tokio::spawn(async move {
            tokio::task::yield_now().await;
            drop(guard);
        });
        // A bound far below any real drain: reaching it means the wake was
        // lost, which is the bug this asserts against.
        let settled = drain.settle(Duration::from_secs(2)).await;
        assert!(settled.is_clean(), "a lost wake-up: {settled:?}");
        releaser.await.expect("the releaser finished");
    }
}

/// A daemon that never bound a listener never calls `attach`, so nobody will
/// ever cancel `stopped`. Waiting for it anyway would hang the whole shutdown
/// — the bound does not cover that wait, it only covers the in-flight count.
#[tokio::test]
async fn a_drain_with_no_accept_loop_settles_without_waiting_for_one() {
    let drain = Drain::new();
    let settled = tokio::time::timeout(GENEROUS, drain.settle(GENEROUS))
        .await
        .expect("a drain with no accept loop settled rather than hanging");
    assert_eq!(settled, Settled::EMPTY);
}

/// `accepting.cancel()` only ASKS the loop to stop. `settle` must wait for the
/// loop to say it has actually left, which is what `stopped_accepting`
/// announces.
#[tokio::test]
async fn settle_waits_for_the_accept_loop_to_actually_leave() {
    let drain = Drain::new();
    drain.attach();
    let left = Arc::new(AtomicBool::new(false));

    let loop_side = tokio::spawn({
        let drain = drain.clone();
        let left = Arc::clone(&left);
        async move {
            drain.accepting().cancelled().await;
            tokio::time::sleep(A_MOMENT).await;
            left.store(true, Ordering::Release);
            drain.stopped_accepting();
        }
    });

    let settled = drain.settle(GENEROUS).await;
    assert!(
        left.load(Ordering::Acquire),
        "settle returned while the accept loop was still running: {settled:?}"
    );
    assert_eq!(settled, Settled::EMPTY);
    loop_side.await.expect("the accept loop finished");
}

/// The bug the handshake exists for: a connection accepted in the window
/// between the cancel and the loop's next poll is real and is being served.
/// Reading the count before the loop has left reports it as zero, and the
/// drain then returns while that request is still in flight.
#[tokio::test]
async fn a_connection_accepted_after_the_cancel_is_counted_at_close() {
    let drain = Drain::new();
    drain.attach();

    let loop_side = tokio::spawn({
        let drain = drain.clone();
        async move {
            drain.accepting().cancelled().await;
            // The last connection the loop took before it noticed.
            let guard = drain.enter();
            drain.stopped_accepting();
            tokio::time::sleep(A_MOMENT).await;
            drop(guard);
        }
    });

    let settled = drain.settle(GENEROUS).await;
    assert_eq!(
        settled.in_flight_at_close, 1,
        "a connection accepted after the cancel went uncounted"
    );
    assert_eq!(settled.abandoned, 0);
    loop_side.await.expect("the accept loop finished");
}

/// The default is the same drain `new` builds: accepting, with nothing in
/// flight. A daemon that takes one by `Default` must not get a drain that is
/// already closed, or its first connection would be refused.
#[tokio::test]
async fn the_default_drain_is_an_open_one() {
    let drain = Drain::default();
    assert!(!drain.accepting().is_cancelled());
    assert_eq!(drain.in_flight(), 0);
    assert_eq!(drain.settle(GENEROUS).await, Settled::EMPTY);
}
