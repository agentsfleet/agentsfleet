//! Dimensions 7.1 and 7.5 — what shutdown promises, proven rather than assumed.
//!
//! Every wait in this file is a handshake, never a sleep. A task reports that
//! it is running on a channel, and the test does not proceed until it has; a
//! sleep would make the same assertions pass on a machine that was merely slow,
//! which is the failure mode a shutdown test is least able to afford.
//!
//! Two of these prove a NEGATIVE — that a task did not stop — and a negative
//! needs a control or it is vacuous. `test_task_inventory_and_cancellation`
//! asserts a blocked `accept()` is interrupted by the token;
//! `test_supervised_accept_completes_when_a_client_connects` is its control,
//! and exists so the first cannot pass by having selected over a branch that
//! could never have fired anyway.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]
#![expect(
    clippy::panic,
    reason = "one task panics deliberately: ShutdownReport has a row for it, and an unreachable row is an unproven one"
)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentsfleetd::{JOIN_TIMEOUT, ShutdownReport, Supervisor};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use self::support::install_subscriber;

/// The tasks the join-order fixture supervises, in the order it spawns them.
const TASK_NAMES: [&str; 3] = ["stream_reader", "session_sweeper", "hub_pump"];

/// The task that owns a listening socket, and is cancelled while blocked on it.
const ACCEPT_TASK: &str = "listener_accept";

/// A task that never selects over its token — the bug Dimension 7.5 catches.
const STUBBORN: &str = "ignores_its_token";

/// A task that ends by panicking, so the report's panic row is not an argument.
const BOOM: &str = "panics_on_poll";

/// How long a shutdown may take before the test calls cancellation broken.
///
/// Deliberately a small fraction of [`JOIN_TIMEOUT`]: if the token did NOT
/// interrupt the blocked accept, `shutdown` would still return — after the full
/// join timeout, reporting the task abandoned. Bounding the wait well under
/// that turns "cancellation works" into a failure the test can actually see,
/// where asserting on the report alone would pass ten seconds later.
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);

/// How a supervised socket owner stopped.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The cancellation token won the race against a blocked `accept()`.
    Cancelled,
    /// A client connected and the accept completed.
    Accepted,
}

/// Dimension 7.1 — every task joins before anything it borrowed can be dropped.
///
/// The `Arc` assertion is the load-bearing one. Invariant C2 is "stop, then
/// join, THEN drop", and the way it fails in practice is a pool closed while a
/// sweeper still holds a connection. Counting strong references after
/// `shutdown` returns is that invariant stated as an observation: if any task
/// were still alive, its clone would still be counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_shutdown_joins_all_tasks() {
    install_subscriber();

    let borrowed = Arc::new(Mutex::new(Vec::new()));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();

    let mut supervisor = Supervisor::new();
    for name in TASK_NAMES {
        let borrowed = Arc::clone(&borrowed);
        let started_tx = started_tx.clone();
        supervisor.spawn(name, move |token| async move {
            started_tx
                .send(name)
                .expect("the test outlives every task it spawns");
            token.cancelled().await;
            borrowed
                .lock()
                .expect("no supervised task panics holding this lock")
                .push(name);
        });
    }
    drop(started_tx);

    // The handshake: every task is RUNNING before anything is cancelled, so a
    // task that stopped did so by observing the token rather than by never
    // having started. Ordering is not asserted here — three tasks reaching a
    // channel is genuinely concurrent, and pinning it would test the scheduler.
    let mut started = Vec::new();
    while started.len() < TASK_NAMES.len() {
        started.push(
            started_rx
                .recv()
                .await
                .expect("a supervised task dropped its handshake sender"),
        );
    }
    started.sort_unstable();
    let mut expected = TASK_NAMES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        started, expected,
        "every supervised task must reach its body"
    );

    let report = supervisor.shutdown().await;

    assert_eq!(
        report,
        ShutdownReport {
            joined: TASK_NAMES.to_vec(),
            abandoned: Vec::new(),
            panicked: Vec::new(),
        },
        "shutdown joins in spawn order, and nothing else happened"
    );
    assert!(report.is_clean(), "a clean shutdown reports itself clean");

    let observed = borrowed
        .lock()
        .expect("no supervised task panics holding this lock")
        .clone();
    assert_eq!(
        observed.len(),
        TASK_NAMES.len(),
        "every task must have observed cancellation, not merely been joined"
    );

    assert_eq!(
        Arc::strong_count(&borrowed),
        1,
        "invariant C2: once shutdown returns, nothing a task borrowed is still held"
    );
}

/// Dimension 7.5 — the inventory is complete, and cancellation interrupts I/O.
///
/// The accept task is the point. A task that merely polls its token between
/// iterations is not cancellable mid-read, and the whole reason the supervisor
/// hands the token out rather than keeping it is so an I/O owner can select
/// over it directly. Nothing ever connects to this listener, so the accept
/// future is genuinely pending when the token fires.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_task_inventory_and_cancellation() {
    install_subscriber();

    assert!(
        SHUTDOWN_BUDGET < JOIN_TIMEOUT,
        "the budget must be able to fail before the join timeout rescues it"
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the loopback interface accepts an ephemeral bind");
    let (outcome_tx, mut outcome_rx) = mpsc::unbounded_channel();
    let (parked_tx, mut parked_rx) = mpsc::unbounded_channel();

    let mut supervisor = Supervisor::new();
    supervisor.spawn(ACCEPT_TASK, move |token| async move {
        parked_tx
            .send(())
            .expect("the test outlives the task it spawns");
        let outcome = tokio::select! {
            () = token.cancelled() => Outcome::Cancelled,
            _ = listener.accept() => Outcome::Accepted,
        };
        outcome_tx
            .send(outcome)
            .expect("the test outlives the task it spawns");
    });
    for name in TASK_NAMES {
        supervisor.spawn(name, |token| async move { token.cancelled().await });
    }

    parked_rx
        .recv()
        .await
        .expect("the socket owner dropped its handshake sender");

    assert!(
        outcome_rx.try_recv().is_err(),
        "nothing connected, so the accept must still be pending when the token fires"
    );

    let mut expected_inventory = vec![ACCEPT_TASK];
    expected_inventory.extend(TASK_NAMES);
    assert_eq!(
        supervisor.inventory(),
        expected_inventory,
        "the inventory names every supervised task, in spawn order"
    );

    let report = tokio::time::timeout(SHUTDOWN_BUDGET, supervisor.shutdown())
        .await
        .expect("cancellation must interrupt a blocked accept, not wait out JOIN_TIMEOUT");

    assert!(
        report.is_clean(),
        "a task selecting over its token stops when asked: {report:?}"
    );
    assert_eq!(
        outcome_rx
            .recv()
            .await
            .expect("the socket owner reports how it stopped"),
        Outcome::Cancelled,
        "the token must win the race against a live accept"
    );
}

/// The control for Dimension 7.5: the accept branch is reachable.
///
/// Without this, `test_task_inventory_and_cancellation` would pass just as
/// happily against a listener that could never produce a connection — a
/// cancellation racing nothing at all. Here a client does connect, the same
/// `select!` takes the other branch, and the negative above becomes a race
/// that was genuinely won rather than one that was never run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_supervised_accept_completes_when_a_client_connects() {
    install_subscriber();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the loopback interface accepts an ephemeral bind");
    let address = listener
        .local_addr()
        .expect("a bound listener knows its address");
    let (outcome_tx, mut outcome_rx) = mpsc::unbounded_channel();

    let mut supervisor = Supervisor::new();
    supervisor.spawn(ACCEPT_TASK, move |token| async move {
        let outcome = tokio::select! {
            () = token.cancelled() => Outcome::Cancelled,
            _ = listener.accept() => Outcome::Accepted,
        };
        outcome_tx
            .send(outcome)
            .expect("the test outlives the task it spawns");
    });

    let client = TcpStream::connect(address)
        .await
        .expect("the supervised listener is accepting");

    assert_eq!(
        outcome_rx
            .recv()
            .await
            .expect("the socket owner reports how it stopped"),
        Outcome::Accepted,
        "with a client connected the accept branch must be the one that fires"
    );
    drop(client);

    let report = supervisor.shutdown().await;
    assert!(
        report.is_clean(),
        "a task that already returned is joined, not abandoned: {report:?}"
    );
}

/// A task that never looks at its token is REPORTED, not waited on forever.
///
/// Time is paused, so the ten-second join timeout costs no wall clock: with
/// every task parked the runtime has nothing to do and advances to the next
/// deadline itself. That is what makes an assertion about a timeout affordable
/// in the unit lane — the alternative is a ten-second test, which is the kind
/// that gets deleted.
#[tokio::test(start_paused = true)]
async fn test_shutdown_reports_a_task_that_ignores_its_token() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    supervisor.spawn(STUBBORN, |_token| std::future::pending());

    let report = supervisor.shutdown().await;

    assert_eq!(
        report,
        ShutdownReport {
            joined: Vec::new(),
            abandoned: vec![STUBBORN],
            panicked: Vec::new(),
        },
        "a task that ignores cancellation is named in the report"
    );
    assert!(
        !report.is_clean(),
        "an abandoned task is not a clean shutdown"
    );
}

/// A task that ends by panicking is reported, and does not fail the shutdown.
///
/// The supervisor owns every handle and never aborts, so a `JoinError` here can
/// only be a panic — which is why the report has a row for it and why that row
/// needs a test rather than an argument.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shutdown_reports_a_panicking_task() {
    install_subscriber();

    let mut supervisor = Supervisor::new();
    supervisor.spawn(BOOM, |_token| async {
        panic!("deliberate: shutdown must REPORT a panicking task, not swallow it")
    });

    let report = supervisor.shutdown().await;

    assert_eq!(
        report,
        ShutdownReport {
            joined: Vec::new(),
            abandoned: Vec::new(),
            panicked: vec![BOOM],
        },
        "a panicking task is named in the report"
    );
    assert!(
        !report.is_clean(),
        "a panicked task is not a clean shutdown"
    );
}

/// A supervisor with nothing running still cancels, and reports nothing.
///
/// The empty case matters because boot can fail before it spawns anything, and
/// the shutdown path runs anyway. Cancelling the token with no tasks to join is
/// what makes that call safe rather than merely harmless.
#[tokio::test]
async fn test_a_new_supervisor_supervises_nothing() {
    install_subscriber();

    let supervisor = Supervisor::default();
    assert!(
        supervisor.inventory().is_empty(),
        "a fresh supervisor supervises nothing"
    );

    let token = supervisor.token();
    assert!(!token.is_cancelled(), "nothing has asked it to stop yet");

    let report = supervisor.shutdown().await;

    assert_eq!(
        report,
        ShutdownReport::default(),
        "an empty shutdown reports an empty result"
    );
    assert!(report.is_clean(), "nothing to join is a clean shutdown");
    assert!(
        token.is_cancelled(),
        "shutdown cancels the token even with nothing to join"
    );
}
