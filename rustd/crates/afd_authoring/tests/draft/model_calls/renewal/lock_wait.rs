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
