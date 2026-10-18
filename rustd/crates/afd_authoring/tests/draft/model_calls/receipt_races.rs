//! Durable physical identities cannot collide, skip calls, or overwrite settled data.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn skipped_indices_and_foreign_dispatch_ids_create_no_physical_intent() {
    let first = Fixture::create().await;
    let a = Attempt::start(&first, first.access(), Source::Blank).await;
    let provider = Arc::new(Provider::known());
    let calls = fixture::calls(&first, &a, Arc::clone(&provider)).await;
    let request = fixture::request(&a);
    let occupied = request.dispatch_id.clone();
    calls
        .call(&first.runner, &a.lease, request)
        .await
        .expect("original dispatch");
    let second = Fixture::create().await;
    let b = Attempt::start(&second, second.access(), Source::Blank).await;
    let calls = fixture::calls(&second, &b, Arc::clone(&provider)).await;
    for (index, id) in [(2, mint().to_string()), (1, occupied)] {
        let mut request = fixture::request(&b);
        request.call_index = index;
        request.dispatch_id = id;
        assert_eq!(
            calls
                .call(&second.runner, &b.lease, request)
                .await
                .expect_err("invalid physical identity")
                .code(),
            error_code::AUTHORING_REPLAY
        );
        assert_eq!(fixture::receipts(&second, &b).await, (0, 0, false));
    }
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&first, &a).await, (1, 1, false));
    first.cleanup().await;
    second.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn removed_or_already_settled_obligations_cannot_be_recreated_by_a_late_reply() {
    for removed in [true, false] {
        let f = Fixture::create().await;
        let a = Attempt::start(&f, f.access(), Source::Blank).await;
        let mut stub = Provider::known();
        stub.blocked = true;
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
        let request = fixture::request(&a);
        let task = tokio::spawn({
            let (runner, lease) = (f.runner.clone(), a.lease.clone());
            async move { calls.call(&runner, &lease, request).await }
        });
        tokio::time::timeout(Duration::from_secs(5), provider.arrived())
            .await
            .expect("physical request reached provider");
        let statement = if removed {
            "DELETE FROM authoring.attempts WHERE id=$1::uuid"
        } else {
            "UPDATE authoring.dispatches SET status='completed',usage='{\"knowledge\":\"known\",\"input_tokens\":1,\"output_tokens\":1,\"provider_cost_nanos\":\"3000\"}'::jsonb WHERE attempt_id=$1::uuid"
        };
        sqlx::query(statement)
            .bind(a.id.as_str())
            .execute(&mut *f.connection().await)
            .await
            .expect("selected concurrent durable change");
        provider.resume();
        let error = task
            .await
            .expect("physical call finished")
            .expect_err("late publication refused");
        assert_eq!(
            error.code(),
            if removed {
                error_code::AUTHORING_NOT_FOUND
            } else {
                error_code::AUTHORING_STATE
            }
        );
        let saved: (i64, i64) = sqlx::query_as("SELECT count(*)::bigint,count(*) FILTER(WHERE result IS NOT NULL OR native_reply IS NOT NULL)::bigint FROM authoring.dispatches WHERE attempt_id=$1::uuid")
            .bind(a.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("durable result state");
        assert_eq!(saved, (i64::from(!removed), 0));
        assert_eq!(provider.count(), 1);
        f.cleanup().await;
    }
}
