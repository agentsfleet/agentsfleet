//! Both run kinds occupy capacity only while their original expiry is live.

use super::Fixture;
use std::time::Duration;

use super::fixture::{LAPSED_BY, LIVE_FOR};
use af_core::clock;
use sqlx::Connection as _;

/// How long the competing assignment holds the host lock.
const LOCK_HELD_FOR: Duration = Duration::from_millis(600);

/// How long the seeded run has left when the wait starts: well inside the
/// hold, so it lapses while the assignment is queued behind the lock.
const LAPSES_IN: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runtime_and_setup_runs_count_together() {
    let fixture = Fixture::create(2).await;
    fixture.runtime(clock::after(clock::now(), LIVE_FOR)).await;
    assert!(
        fixture.try_assign().await,
        "one remaining worker accepts setup"
    );
    assert!(
        !fixture.try_assign().await,
        "runtime plus setup exhaust the host"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_expired_status_active_rows_do_not_occupy_capacity() {
    let fixture = Fixture::create(1).await;
    fixture
        .runtime(clock::before(clock::now(), LAPSED_BY))
        .await;
    let mut connection = fixture.db.acquire().await.expect("setup seed connection");
    let mut tx = connection
        .begin()
        .await
        .expect("expired setup run transaction");
    fixture
        .setup(&mut tx, clock::before(clock::now(), LAPSED_BY))
        .await;
    tx.commit()
        .await
        .expect("expired status-active setup run seeds");
    drop(connection);
    assert!(
        fixture.try_assign().await,
        "expiry alone frees the worker before sweep"
    );
    fixture.cleanup().await;
}

/// A run that lapsed while the assignment waited on the host
/// lock no longer occupies the host: the census reads the clock once the lock
/// is held, never at the instant the assignment asked for it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_expiry_checks_read_clock_after_lock() {
    let fixture = Fixture::create(1).await;
    // Seeded long-lived, so the refusal below cannot race the expiry: a slow
    // run (coverage instrumentation) once let a 200 ms run lapse before the
    // first assignment finished, and that assignment then succeeded.
    fixture.runtime(clock::after(clock::now(), LIVE_FOR)).await;
    assert!(
        !fixture.try_assign().await,
        "the live run fills the only worker"
    );

    let mut holder = fixture.db.acquire().await.expect("competing connection");
    let mut held = holder.begin().await.expect("competing transaction");
    fixture.lock_host(&mut held).await;
    // Only now, with the host held, is the run set to lapse inside the hold.
    fixture.lapse_runtime_in(LAPSES_IN).await;
    let release = async move {
        tokio::time::sleep(LOCK_HELD_FOR).await;
        held.rollback()
            .await
            .expect("the competing assignment lets go");
    };
    let (assigned, ()) = tokio::join!(fixture.try_assign(), release);

    assert!(
        assigned,
        "the run lapsed during the wait, so the census must not count it"
    );
    drop(holder);
    fixture.cleanup().await;
}
