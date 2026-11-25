//! A Stop and an issue meet on the same thread. A Stop that committed before the
//! issue's insert, or while the insert waited on the thread, leaves no run; a
//! Stop that waited behind the issue names and marks the run it issued. These
//! issue as delivery does, which reads no task source for a plain turn and so
//! leaves the insert as the only check.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_fleet::run::{Acquired, Billed};
use afd_turns::view::History;
use afd_turns::view::stop::{Stop, hold_thread, stop_on};
use sqlx::Connection as _;

use crate::seed::{self, MODEL, POSTURE, PROVIDER, Seeded, seeded};
use crate::support::Fixtures;

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// How many runs name the turn, whatever their status.
const RUNS_FOR_TURN: &str = "SELECT count(*) FROM thread.runs WHERE turn_id = $1";

/// Whether the run carries Stop's durable cancel mark.
const CANCEL_MARKED: &str = "SELECT cancel_requested_at IS NOT NULL FROM thread.runs WHERE id = $1";

/// The placement version the issue's insert compares.
const RUN_VERSION: &str = "SELECT run_version FROM runner.runners WHERE id = $1";

/// Whether `work` queues behind a lock `pid` holds before it finishes; `work` keeps
/// running either way, so a statement that never waits fails the test, not hangs it.
async fn queues_behind<F: Future + Unpin>(database: &afd_db::Db, pid: i32, work: &mut F) -> bool {
    let mut probe = database.acquire().await.expect("a connection");
    let waiter = async {
        loop {
            let queued: bool = sqlx::query_scalar(BLOCKING)
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .expect("inspect the lock queue");
            if queued {
                return;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::select! {
        _ = work => false,
        () = waiter => true,
    }
}

async fn runs_for_turn(database: &afd_db::Db, turn: &Uuid7) -> i64 {
    let mut probe = database.acquire().await.expect("a connection");
    sqlx::query_scalar(RUNS_FOR_TURN)
        .bind(turn)
        .fetch_one(&mut *probe)
        .await
        .expect("count the turn's runs")
}

fn billed(tenant: &Uuid7) -> Billed<'_> {
    Billed {
        tenant_id: tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    }
}

/// The control for the two refusals below: the same fixture, with nothing
/// stopped, issues the head.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_head_nothing_stopped_is_issued() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    let issued = runs
        .issue(&first, &held, billed(&tenant), fixtures.enrolled_at())
        .await
        .expect("the issue answers");

    assert!(issued.is_some(), "an unstopped head is issued");
    assert_eq!(runs_for_turn(&fixtures.database, &held.turn_id).await, 1);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_head_stopped_after_its_assignment_is_not_issued() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    let stopped = History::new(fixtures.database.clone())
        .stop(&held.workspace_id, &held.fleet_id, &held.turn_id)
        .await
        .expect("Stop answers")
        .expect("the turn exists");
    assert!(
        matches!(stopped.stop, Stop::Cancelled { running: false, .. }),
        "no run served the turn when Stop committed: {:?}",
        stopped.stop
    );
    let issued = runs
        .issue(&first, &held, billed(&tenant), fixtures.enrolled_at())
        .await
        .expect("the issue answers");
    assert!(issued.is_none(), "a stopped head is not issued");
    assert_eq!(runs_for_turn(&fixtures.database, &held.turn_id).await, 0);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stop_committed_while_the_issue_waits_issues_no_run() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");
    let mut holder = fixtures.database.acquire().await.expect("a connection");
    let mut stopping = holder.begin().await.expect("Stop's transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *stopping)
        .await
        .expect("Stop's backend");
    hold_thread(&mut stopping, &held.workspace_id, &held.fleet_id)
        .await
        .expect("Stop holds the fleet and its thread");

    let mut issue = Box::pin(runs.issue(&first, &held, billed(&tenant), fixtures.enrolled_at()));
    assert!(
        queues_behind(&fixtures.database, pid, &mut issue).await,
        "the issue waits on the thread Stop holds"
    );
    let stopped = stop_on(
        &mut stopping,
        &held.workspace_id,
        &held.fleet_id,
        &held.turn_id,
    )
    .await
    .expect("Stop's statement")
    .expect("the turn exists");
    stopping.commit().await.expect("Stop commits");
    let issued = issue.await;

    assert!(matches!(
        stopped.stop,
        Stop::Cancelled { running: false, .. }
    ));
    let issued = issued.expect("the issue answers");
    assert!(issued.is_none(), "the issue re-read the turn Stop settled");
    assert_eq!(runs_for_turn(&fixtures.database, &held.turn_id).await, 0);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_stop_that_waited_behind_an_issue_marks_its_run() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");

    let mut issuer = fixtures.database.acquire().await.expect("a connection");
    let mut issuing = issuer.begin().await.expect("the issue's transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *issuing)
        .await
        .expect("the issuer's backend");
    let run_id = mint_id();
    insert_run(&mut issuing, &held, &first, &tenant, &run_id).await;

    let history = History::new(fixtures.database.clone());
    let mut stop = Box::pin(history.stop(&held.workspace_id, &held.fleet_id, &held.turn_id));
    assert!(
        queues_behind(&fixtures.database, pid, &mut stop).await,
        "Stop waits on the thread the issue holds"
    );
    issuing.commit().await.expect("the issue commits");

    let stopped = stop.await.expect("Stop answers").expect("the turn exists");
    let Stop::Cancelled { running, runs, .. } = stopped.stop else {
        panic!("Stop cancels the turn: {:?}", stopped.stop);
    };
    assert!(running, "the run the issue committed serves the turn");
    assert_eq!(
        runs.iter().map(|marked| &marked.run).collect::<Vec<_>>(),
        vec![&run_id]
    );
    let mut probe = fixtures.database.acquire().await.expect("a connection");
    let marked: bool = sqlx::query_scalar(CANCEL_MARKED)
        .bind(&run_id)
        .fetch_one(&mut *probe)
        .await
        .expect("read the run's mark");
    assert!(marked, "the run carries Stop's cancel mark");
    fixtures.cleanup().await;
}

/// The issue's run insert, on a transaction the test keeps open.
async fn insert_run(
    connection: &mut sqlx::PgConnection,
    held: &Acquired,
    runner: &Uuid7,
    tenant: &Uuid7,
    run_id: &Uuid7,
) {
    let run_version: i64 = sqlx::query_scalar(RUN_VERSION)
        .bind(runner)
        .fetch_one(&mut *connection)
        .await
        .expect("the runner's placement version");
    let event_row_id = mint_id();
    let inserted = afd_fleet::run::sql::run::RunRow {
        run_id,
        runner_id: runner,
        fleet_id: &held.fleet_id,
        workspace_id: &held.workspace_id,
        tenant_id: tenant,
        turn_id: &held.turn_id,
        turn_created_at: held.turn_created_at,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
        fencing_token: held.fence.as_i64(),
        status: afd_state::sql::RUN_STATUS_ACTIVE,
        event_row_id: &event_row_id,
        kind: held.kind.as_str(),
        decision_snapshot: None,
        run_version,
        held_until: held.held_until,
    }
    .insert(connection)
    .await
    .expect("the run insert answers");
    assert!(inserted.is_some(), "the assignment is still held");
}
