//! A lock wait cannot preserve the pre-wait clock for renewal.
//!
//! Renewal's lock is the attempt row (money moves through the biller in its
//! own transactions, so no wallet row is locked here): a renewal that waited on
//! it must read the clock after the wait, and refuse a lease that expired
//! meanwhile.

use super::super::fixture::{self, Provider};
use super::{Attempt, EXPIRE_NOW, Fixture, request, snapshot};
use afd_authoring::types::Source;
use afd_core::error_code;
use sqlx::Connection as _;
use std::{sync::Arc, time::Duration};

const BLOCKED: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn expiry_during_observed_attempt_wait_denies_renewal_without_meter_advance() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let before = snapshot(&f, &a).await;
    let mut connection = f.connection().await;
    let mut held = connection.begin().await.expect("attempt blocker");
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM authoring.attempts WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(a.id.as_str())
    .fetch_one(&mut *held)
    .await
    .expect("own attempt lock");
    let expire = async {
        tokio::time::timeout(Duration::from_secs(5), blocked(&f, pid))
            .await
            .expect("renewal actually waits");
        sqlx::query(EXPIRE_NOW)
            .bind(a.id.as_str())
            .execute(&mut *held)
            .await
            .expect("expiry wins while waiting");
        held.commit().await.expect("release attempt");
    };
    let (renewed, ()) = tokio::join!(f.drafts.renew(&f.runner, &a.lease, request()), expire);
    assert_eq!(
        renewed.expect_err("wait cannot revive expiry").code(),
        error_code::AUTHORING_FENCE
    );
    let after = snapshot(&f, &a).await;
    assert!(after.0 < before.0);
    assert_eq!((after.1, after.2), (before.1, before.2));
    drop(connection);
    f.cleanup().await;
}

async fn blocked(f: &Fixture, pid: i32) {
    let mut connection = f.connection().await;
    loop {
        if sqlx::query_scalar::<_, bool>(BLOCKED)
            .bind(pid)
            .fetch_one(&mut *connection)
            .await
            .expect("inspect attempt wait")
        {
            return;
        }
        tokio::task::yield_now().await;
    }
}

/// Leaves the attempt's lease one second live on database time.
const ONE_SECOND_LEFT: &str = "UPDATE authoring.attempts \
SET lease_expires_at=date_trunc('milliseconds',clock_timestamp())+interval '1 second' \
WHERE id=$1::uuid";

// The expiry is read once, before the hold, and the wait compares it with the
// database clock only: polling `authoring.attempts` while the hold stands would
// queue behind any other test's DDL on that table, which itself waits on this
// renewal — a cross-test wait Postgres cannot see as a deadlock.
const EXPIRES: &str = "SELECT lease_expires_at::text FROM authoring.attempts WHERE id=$1::uuid";
/// Whether the attempt's lease has run out on database time.
const LAPSED: &str = "SELECT $1::timestamptz <= clock_timestamp()";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn expiry_while_the_model_check_waits_denies_the_renewal_at_its_final_clock_read() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    sqlx::query(ONE_SECOND_LEFT)
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("a lease about to lapse");
    let before = snapshot(&f, &a).await;
    let expires: String = sqlx::query_scalar(EXPIRES)
        .bind(a.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("read the lease expiry");
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    // The model check reads the owner's model FOR SHARE, after the guard has
    // already passed the still-live lease.
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM model.tenant_models WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(f.model.as_str())
    .fetch_one(&mut *held)
    .await
    .expect("hold the owner's model");
    let (renewed, ()) = tokio::join!(f.drafts.renew(&f.runner, &a.lease, request()), async {
        f.waits_on(pid, 1).await;
        while !sqlx::query_scalar::<_, bool>(LAPSED)
            .bind(expires.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("read the lease")
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        held.rollback().await.expect("release the model");
    });
    drop(holder);
    assert_eq!(
        renewed
            .expect_err("the lease lapsed before the final clock read")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(
        snapshot(&f, &a).await,
        before,
        "nothing extended or metered"
    );
    f.cleanup().await;
}

/// Takes the runner's host row only if nobody holds it.
const TAKE_HOST_NOW: &str = "SELECT id FROM runner.runners WHERE id=$1::uuid FOR UPDATE NOWAIT";

/// A renewal that waits on a busy draft waits holding nothing of the
/// runner's: the host row stays free for the runner's other placements, and
/// the renewal still succeeds once the draft is released.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn test_a_renewal_waiting_on_a_busy_draft_leaves_the_host_free() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(Provider::known())).await;
    let mut connection = f.connection().await;
    let mut held = connection.begin().await.expect("draft holder");
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(a.draft.id.as_str())
    .fetch_one(&mut *held)
    .await
    .expect("hold the draft");
    let probe = async {
        tokio::time::timeout(Duration::from_secs(5), blocked(&f, pid))
            .await
            .expect("the renewal waits on the draft");
        let mut session = f.connection().await;
        let mut taken = session.begin().await.expect("host probe");
        let host = sqlx::query(TAKE_HOST_NOW)
            .bind(f.runner.as_str())
            .fetch_optional(&mut *taken)
            .await;
        taken.rollback().await.expect("release the probe");
        held.commit().await.expect("release the draft");
        host
    };
    let (renewed, host) = tokio::join!(f.drafts.renew(&f.runner, &a.lease, request()), probe);
    host.expect("the host row was free while the renewal waited");
    renewed.expect("the renewal completes once the draft is free");
    drop(connection);
    f.cleanup().await;
}
