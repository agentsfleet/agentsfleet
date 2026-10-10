//! What the two recovery passes do when another transaction holds their row.
//!
//! The reconcile pass reads unlocked and pins the receipt it probed on its
//! write; the replay pass locks its batch and skips what someone else holds.
//! Both claims are about a second transaction, so each test here IS that
//! second transaction: it takes the admission row's lock itself, waits until
//! the pass under test is parked behind it (or proves it never parks), and
//! changes the row underneath before letting go.
//!
//! Serialised on [`RECOVERY_LANE`] with every other suite that runs a
//! deployment-wide pass (ISO-3).

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_admission::Progress;
use afd_core::clock;
use afd_dragonfly::FleetStreams;
use sqlx::{Connection as _, Row as _};

use crate::integration_admission_recovery::{
    EVERY_FLEET, EVERY_ROW, NO_GRACE, RECOVERY_LANE, admission, deferring, ledger, producer_key,
};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// Takes one admission row's lock inside the caller's transaction.
const LOCK_ROW: &str = "SELECT 1 FROM core.fleet_admissions WHERE id = $1::uuid FOR UPDATE";

/// Rewrites a held row's receipt, as a replay on another replica would.
const RERECEIPT: &str = "UPDATE core.fleet_admissions SET receipt = $2 WHERE id = $1::uuid";

/// This connection's server process, which a blocked statement names.
const BACKEND_PID: &str = "SELECT pg_backend_pid()";

/// Whether any statement is waiting on a lock the given backend holds.
const SOMEONE_WAITS_ON: &str =
    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))";

/// How long a pass may take to park behind the held row, or to finish past it.
const PATIENCE: Duration = Duration::from_secs(20);

/// How often the waiter looks.
const POLL: Duration = Duration::from_millis(25);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reconcile_void_lands_only_on_the_receipt_it_probed() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let live = ledger(&fixtures);
    let key = producer_key(&fleet, "pinned-void");
    let event = live
        .admit(admission(&fleet, &workspace, &key))
        .await
        .expect("a live queue admits and receipts")
        .stored
        .id;
    FleetStreams::new(fixtures.queue().clone())
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");

    let mut held_connection = fixtures.database.acquire().await.expect("a connection");
    let mut holder = held_connection
        .begin()
        .await
        .expect("a holding transaction");
    sqlx::query(LOCK_ROW)
        .bind(&event)
        .execute(&mut *holder)
        .await
        .expect("the row locks");
    let pid: i32 = sqlx::query(BACKEND_PID)
        .fetch_one(&mut *holder)
        .await
        .expect("the backend answers")
        .get(0);
    let replaced = format!("{event}-replayed");

    // The pass reads the row WITHOUT waiting, probes the lost receipt, and only
    // its void parks behind the lock. While it is parked, the row's receipt
    // changes and commits; the void then re-reads the row and must find the
    // receipt it was told about gone.
    let (lane_view, row, receipt) = (&fixtures, event.as_str(), replaced.as_str());
    let rereceipt = async move {
        wait_until_blocked(lane_view, pid).await;
        sqlx::query(RERECEIPT)
            .bind(row)
            .bind(receipt)
            .execute(&mut *holder)
            .await
            .expect("the held row takes its new receipt");
        holder
            .commit()
            .await
            .expect("the holding transaction commits");
    };
    let mut progress = Progress::default();
    let (reconciled, ()) = tokio::join!(
        live.reconcile(clock::now(), EVERY_FLEET, EVERY_ROW, &mut progress),
        rereceipt,
    );
    reconciled.expect("the pass runs against both live datastores");

    assert_eq!(
        fixtures.admission_receipt(&fleet, &event).await,
        Some(replaced),
        "a void probed for one receipt must not forget the one that replaced it"
    );

    // Leave the row whole for whoever runs next: the replacement names no
    // entry either, so the production passes repair it the ordinary way.
    live.reconcile(
        clock::now(),
        EVERY_FLEET,
        EVERY_ROW,
        &mut Progress::default(),
    )
    .await
    .expect("the repairing pass runs");
    live.replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs");
    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replay_pass_skips_the_row_another_replica_holds() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, _runners) = seeded_parts::<0>(&fixtures).await;
    let mut owed = Vec::new();
    for which in ["held", "free"] {
        let key = producer_key(&fleet, which);
        owed.push(
            deferring(&fixtures)
                .admit(admission(&fleet, &workspace, &key))
                .await
                .expect("the row commits whatever the queue does")
                .stored
                .id,
        );
    }
    let [held, free] = <[String; 2]>::try_from(owed).expect("two rows were admitted");

    let mut held_connection = fixtures.database.acquire().await.expect("a connection");
    let mut holder = held_connection
        .begin()
        .await
        .expect("a holding transaction");
    sqlx::query(LOCK_ROW)
        .bind(&held)
        .execute(&mut *holder)
        .await
        .expect("the row locks");

    tokio::time::timeout(
        PATIENCE,
        ledger(&fixtures).replay(clock::now(), NO_GRACE, EVERY_ROW),
    )
    .await
    .expect("a replay pass never waits on a row another replica holds")
    .expect("the replay pass runs against both live datastores");

    assert!(
        fixtures.admission_receipt(&fleet, &free).await.is_some(),
        "the free row was replayed"
    );
    assert_eq!(
        fixtures.admission_receipt(&fleet, &held).await,
        None,
        "the held row was left to the replica holding it"
    );

    holder
        .rollback()
        .await
        .expect("the holding transaction ends");
    ledger(&fixtures)
        .replay(clock::now(), NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs");
    assert!(
        fixtures.admission_receipt(&fleet, &held).await.is_some(),
        "once released, the held row is replayed by the next pass"
    );
    fixtures.cleanup().await;
}

/// Returns once a statement is parked behind a lock `pid` holds.
async fn wait_until_blocked(fixtures: &Fixtures, pid: i32) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let mut connection = fixtures.database.acquire().await.expect("a connection");
        let waiting: bool = sqlx::query(SOMEONE_WAITS_ON)
            .bind(pid)
            .fetch_one(&mut *connection)
            .await
            .expect("the activity view answers")
            .get(0);
        if waiting {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the pass never reached the held row within {PATIENCE:?}"
        );
        tokio::time::sleep(POLL).await;
    }
}
