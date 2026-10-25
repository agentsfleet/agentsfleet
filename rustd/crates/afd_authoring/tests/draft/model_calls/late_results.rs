//! Stop/deletion finish while the physical call waits; late usage creates no private copy.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::{sync::Arc, time::Duration};

/// How long a step may take before the test calls it a hang.
///
/// The provider stays blocked until both steps finish, so a lock held across
/// the network call never lets them finish at all: this bound only turns that
/// hang into a failure. It is not a performance budget, which is why it is
/// generous enough for a loaded lane.
const HANG_BOUND: Duration = Duration::from_secs(60);

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn stop_before_reply_records_known_usage_without_publishing_late_text() {
    late_reply(false).await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn deletion_before_reply_cannot_restore_private_dispatch_content() {
    late_reply(true).await;
}

async fn late_reply(delete: bool) {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(blocked_proposal());
    let calls = fixture::calls(&f, &attempt, std::sync::Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let task = tokio::spawn({
        let (runner, lease) = (f.runner.clone(), attempt.lease.clone());
        async move { calls.call(&runner, &lease, request).await }
    });
    tokio::time::timeout(HANG_BOUND, provider.arrived())
        .await
        .expect("physical effect observed");
    tokio::time::timeout(HANG_BOUND, mutate(&f, &attempt, delete))
        .await
        .expect("no locks held across network");
    provider.resume();
    let code = task
        .await
        .expect("late reply task")
        .expect_err("late text denied")
        .code();
    assert_eq!(
        code,
        if delete {
            error_code::AUTHORING_NOT_FOUND
        } else {
            error_code::AUTHORING_FENCE
        }
    );
    let saved: (bool,String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("late accounting receipt");
    assert_eq!(saved, (true, "known".into()));
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    assert_eq!(f.counts().await, if delete { (0, 0, 0) } else { (1, 0, 1) });
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

fn blocked_proposal() -> Provider {
    let mut stub = Provider::known();
    let proposal = serde_json::json!({"kind":"initialize","definition":super::super::definition()});
    stub.response.as_object_mut().expect("response object").insert("output".into(), serde_json::json!([
        {"type":"reasoning","id":"rs_private","summary":[],"encrypted_content":"opaque-private-reasoning"},
        {"type":"function_call","id":"fc_private","call_id":"call_proposal","name":"propose_setup",
         "arguments":serde_json::json!({"proposal":proposal}).to_string()}
    ]));
    stub.blocked = true;
    stub
}

async fn mutate(f: &Fixture, attempt: &Attempt, delete: bool) {
    if delete {
        f.drafts
            .delete(f.access(), &attempt.draft.id)
            .await
            .expect("delete commits without provider locks");
    } else {
        f.drafts
            .cancel(f.access(), &attempt.draft.id, 1)
            .await
            .expect("Stop commits without provider locks");
    }
}
