//! Database time after authorization belongs to the same physical-send budget.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use sqlx::{Connection as _, PgConnection};
use std::{sync::Arc, time::Duration};

const INSTALL: [&str; 2] = [
    "CREATE FUNCTION public.hold_dispatch() RETURNS trigger LANGUAGE plpgsql AS $$ \
     BEGIN PERFORM pg_advisory_xact_lock(hashtext(NEW.id::text)::bigint); RETURN NEW; END $$",
    "CREATE TRIGGER hold_dispatch BEFORE INSERT ON authoring.dispatches \
     FOR EACH ROW EXECUTE FUNCTION public.hold_dispatch()",
];

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn elapsed_authorization_budget_records_deadline_without_sending_or_replaying_work() {
    let f = Fixture::isolated().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let (mut owner, pid) = hold(&f, &request.dispatch_id).await;
    let task = tokio::spawn({
        let (calls, runner, lease, request) = (
            calls.clone(),
            f.runner.clone(),
            attempt.lease.clone(),
            request.clone(),
        );
        async move { calls.call(&runner, &lease, request).await }
    });
    f.waits_on(pid, 1).await;
    assert_eq!(
        provider.count(),
        0,
        "no physical effect before committed intent"
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(301)).await;
    tokio::time::resume();
    sqlx::query("ROLLBACK")
        .execute(&mut owner)
        .await
        .expect("release delayed intent");
    let error = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("elapsed call finishes")
        .expect("dispatch task")
        .expect_err("elapsed authorization cannot send");
    assert_eq!(error.code(), error_code::AUTHORING_UNKNOWN);
    deadline_receipt(&f, &request.dispatch_id).await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect_err("unknown replay remains fenced")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    assert_eq!(provider.count(), 0);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, true));
    owner.close().await.expect("fault session closed");
    f.cleanup().await;
}

async fn hold(f: &Fixture, dispatch: &str) -> (PgConnection, i32) {
    let mut owner = PgConnection::connect(&f.owner_url())
        .await
        .expect("fault owner");
    for statement in INSTALL {
        sqlx::query(statement)
            .execute(&mut owner)
            .await
            .expect("hold dispatch insertion");
    }
    sqlx::query("BEGIN")
        .execute(&mut owner)
        .await
        .expect("hold authorization commit");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
        .bind(dispatch)
        .execute(&mut owner)
        .await
        .expect("own dispatch hold");
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut owner)
        .await
        .expect("hold identity");
    (owner, pid)
}

async fn deadline_receipt(f: &Fixture, dispatch: &str) {
    let receipt: (String, String, bool) = sqlx::query_as(
        "SELECT status,failure_cause,result IS NULL AND native_reply IS NULL FROM authoring.dispatches WHERE id=$1::uuid")
        .bind(dispatch).fetch_one(&mut *f.connection().await).await.expect("content-free deadline receipt");
    assert_eq!(receipt, ("unknown".into(), "deadline".into(), true));
}
