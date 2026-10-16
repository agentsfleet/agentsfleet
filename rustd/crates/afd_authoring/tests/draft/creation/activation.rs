//! Activation and the initial admission commit as one local state change.

#![expect(
    clippy::panic,
    reason = "a start outcome of the wrong shape fails the proof loudly"
)]

use super::super::mint;
use super::fixture::{Case, NO_ADMISSIONS};
use afd_authoring::types::{DraftState, Start, StartOutcome};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn save_without_run_activates_without_admitting_a_message() {
    let c = Case::create().await;
    c.fund().await;
    let (draft, request) = c.request("save-without-run", Start::None).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    assert_eq!(saved.status, "active");
    assert!(matches!(saved.start, StartOutcome::None));
    assert_eq!(saved.setup.state, "ready");
    assert_eq!(c.admissions(&saved.id).await, NO_ADMISSIONS);
    assert_eq!(
        afd_dragonfly::test_util::fleet_entries(&c.queue, saved.id.as_str())
            .await
            .expect("stream"),
        [] as [afd_dragonfly::FleetEvent; 0]
    );
    let closed = c
        .base
        .drafts
        .read(c.base.access(), &draft)
        .await
        .expect("closed draft retained privately");
    assert_eq!(closed.state, DraftState::Closed);
    assert_eq!(c.save(&key, request).await.id, saved.id);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn run_now_is_accepted_only_with_one_saved_receipt_and_replays_same_event() {
    let c = Case::create().await;
    c.fund().await;
    let (_, request) = c
        .request(
            "save-and-run",
            Start::RunNow {
                message: "Run this now".to_owned(),
            },
        )
        .await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    assert_eq!(saved.status, "active");
    let StartOutcome::RunNow {
        state,
        event_id,
        error_code,
    } = &saved.start
    else {
        panic!("run outcome");
    };
    assert_eq!(*state, afd_authoring::types::StartState::Accepted);
    assert!(error_code.is_none());
    let rows = c.admissions(&saved.id).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(event_id.as_ref(), Some(&rows[0].0));
    assert!(rows[0].1.is_some());
    let retried = c.save(&key, request).await;
    assert_eq!(retried.id, saved.id);
    assert_eq!(
        serde_json::to_value(retried.start).expect("outcome"),
        serde_json::to_value(&saved.start).expect("outcome")
    );
    assert_eq!(c.admissions(&saved.id).await, rows);
    assert_eq!(
        afd_dragonfly::test_util::fleet_entries(&c.queue, saved.id.as_str())
            .await
            .expect("stream")
            .len(),
        1
    );
    c.cleanup().await;
}
