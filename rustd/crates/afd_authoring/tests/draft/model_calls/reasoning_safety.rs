//! A native reply cannot escape its original history, retention or authority.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture,
    reasoning::Reasoning,
};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelMessage};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
enum Changed {
    Prefix,
    Assistant,
    Omitted,
    Expired,
    Oversized,
    Malformed,
    Revoked,
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn invalid_native_replays_never_create_another_physical_dispatch() {
    for changed in [
        Changed::Prefix,
        Changed::Assistant,
        Changed::Omitted,
        Changed::Expired,
        Changed::Oversized,
        Changed::Malformed,
        Changed::Revoked,
    ] {
        refused(changed).await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn expired_native_state_is_purged_even_when_the_public_result_is_already_absent() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Reasoning::default());
    let calls = fixture::calls(&f, &attempt, provider).await;
    calls
        .call(&f.runner, &attempt.lease, fixture::request(&attempt))
        .await
        .expect("native proposal");
    let before: bool = sqlx::query_scalar("UPDATE fleet.authoring_dispatches SET result=NULL,private_expires_at=0 WHERE attempt_id=$1::uuid RETURNING native_reply IS NOT NULL")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("retained native state");
    assert!(before);
    f.drafts.maintain().await.expect("private retention sweep");
    let kept: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM fleet.authoring_dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("accounting survives private purge");
    assert_eq!(kept, (true, "known".into()));
    f.cleanup().await;
}

async fn refused(changed: Changed) {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Reasoning::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let mut next = fixture::request(&attempt);
    let first = calls
        .call(&f.runner, &attempt.lease, next.clone())
        .await
        .expect("native proposal");
    next.call_index = 2;
    next.dispatch_id = mint().to_string();
    next.request.messages.push(ModelMessage::Assistant {
        text: first.text,
        proposal_calls: first.proposal_calls,
    });
    next.request.messages.push(ModelMessage::Tool {
        call_id: "call_proposal".into(),
        text: "Accepted".into(),
    });
    mutate(&f, &attempt, &mut next, changed).await;
    let failure = calls
        .call(&f.runner, &attempt.lease, next)
        .await
        .expect_err("native state must be refused");
    if matches!(
        changed,
        Changed::Prefix | Changed::Assistant | Changed::Omitted
    ) {
        assert_eq!(failure.code(), error_code::AUTHORING_REPLAY, "{changed:?}");
    }
    assert_eq!(
        provider.bodies.lock().expect("provider recording").len(),
        1,
        "{changed:?}"
    );
    assert_eq!(
        fixture::receipts(&f, &attempt).await,
        (1, 1, false),
        "{changed:?}"
    );
    f.cleanup().await;
}

async fn mutate(
    f: &Fixture,
    attempt: &Attempt,
    request: &mut afd_authoring::ModelCallInput,
    changed: Changed,
) {
    match changed {
        Changed::Prefix => {
            *request
                .request
                .messages
                .first_mut()
                .expect("opening message") = ModelMessage::User {
                text: "Another conversation".into(),
            }
        }
        Changed::Assistant => {
            let message = request
                .request
                .messages
                .get_mut(1)
                .expect("assistant message");
            assert!(matches!(message, ModelMessage::Assistant { .. }));
            if let ModelMessage::Assistant { text, .. } = message {
                text.push_str("changed");
            }
        }
        Changed::Omitted => {
            request.request.messages.remove(1);
        }
        Changed::Expired => {
            sqlx::query("UPDATE fleet.authoring_dispatches SET private_expires_at=0 WHERE attempt_id=$1::uuid")
                .bind(attempt.id.as_str()).execute(&mut *f.connection().await).await.expect("expired private state");
        }
        Changed::Oversized | Changed::Malformed => {
            corrupt_native_reply(f, attempt, changed).await;
        }
        Changed::Revoked => {
            sqlx::query("DELETE FROM core.tenant_model_entries WHERE id=$1::uuid")
                .bind(f.model.as_str())
                .execute(&mut *f.connection().await)
                .await
                .expect("revoked model");
        }
    }
}

async fn corrupt_native_reply(f: &Fixture, attempt: &Attempt, changed: Changed) {
    let value = if matches!(changed, Changed::Oversized) {
        serde_json::json!({"overflow":"x".repeat(ENVELOPE_BYTES_MAX + 1)})
    } else {
        serde_json::json!({"invalid":"native state"})
    };
    sqlx::query("UPDATE fleet.authoring_dispatches SET native_reply=$2 WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str())
        .bind(value)
        .execute(&mut *f.connection().await)
        .await
        .expect("corrupted private storage");
}
