//! Recovery actions cannot be invented by an authenticated runner.

use super::{Attempt, Fixture, accounting, counts, fixture, messages, report};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{FailureReason, Outcome, Report};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn reports_cannot_invent_owner_actions_or_an_accounting_only_cancellation() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(fixture::Provider::known())).await;
    for reason in [
        FailureReason::ModelUnavailable,
        FailureReason::InsufficientBalance,
        FailureReason::ContextTooLarge,
        FailureReason::PolicyChanged,
    ] {
        let mut request = report();
        let Report::Terminal { accounting, .. } = &mut request else {
            unreachable!()
        };
        accounting.outcome = Outcome::Failed;
        accounting.failure_reason = Some(reason);
        assert_eq!(
            f.drafts
                .report(&f.runner, &a.lease, request)
                .await
                .expect_err("a caller label is not daemon evidence")
                .code(),
            error_code::INVALID_REQUEST
        );
    }
    let mut request = accounting(&report());
    let Report::Accounting { accounting } = &mut request else {
        unreachable!()
    };
    accounting.outcome = Outcome::Cancelled;
    assert_eq!(
        f.drafts
            .report(&f.runner, &a.lease, request)
            .await
            .expect_err("missing saved cancellation fence")
            .code(),
        error_code::INVALID_REQUEST
    );
    assert_eq!(counts(&f, &a).await, (0, 0, 0));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_revoked_model_is_durable_and_accounting_cannot_restore_its_answer() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let calls = fixture::calls(&f, &a, Arc::new(fixture::Provider::known())).await;
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("revoke model");
    assert_eq!(
        calls
            .call(&f.runner, &a.lease, fixture::request(&a))
            .await
            .expect_err("current owner model is unavailable")
            .code(),
        error_code::AUTHORING_MODEL
    );
    let before = messages(&f, &a).await;
    let failure = before
        .last()
        .expect("durable failed reply")
        .failure
        .as_ref()
        .expect("failure");
    assert_eq!(failure.reason, FailureReason::ModelUnavailable);
    assert_eq!(failure.recovery_action, "choose_authoring_model");
    let ack = f
        .drafts
        .report(&f.runner, &a.lease, report())
        .await
        .expect("accounting settles");
    assert!(ack.settled);
    assert_eq!(ack.state, Outcome::Failed);
    assert_eq!(counts(&f, &a).await.2, 0);
    assert!(
        messages(&f, &a)
            .await
            .last()
            .expect("saved failure")
            .text
            .is_none()
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_forged_capability_cannot_write_failure_evidence_even_after_revocation() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let calls = fixture::calls(&f, &a, Arc::new(fixture::Provider::known())).await;
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("revoke model");
    let mut request = fixture::request(&a);
    request.capability_token = "b".repeat(43);
    assert_eq!(
        calls
            .call(&f.runner, &a.lease, request)
            .await
            .expect_err("capability check precedes model lookup")
            .code(),
        error_code::AUTHORING_FENCE
    );
    let reason: Option<String> =
        sqlx::query_scalar("SELECT failure_reason FROM authoring.attempts WHERE id=$1::uuid")
            .bind(a.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("saved authority");
    assert!(reason.is_none());
    f.cleanup().await;
}
