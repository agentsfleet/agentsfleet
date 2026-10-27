//! Already-open tails learn about committed changes without reconnecting.

use super::{fixture::*, streams::Reader};
use afd_core::id::Uuid7;
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn access_changes_notify_only_dependent_fleets_in_the_owning_workspace() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    let other = Saved::create(json!({"credentials":[SERVICE]})).await;
    let ([dependent, trigger, unrelated], mut readers) = access_readers(&saved, &other).await;
    for (method, status) in [
        (Method::POST, StatusCode::CREATED),
        (Method::PUT, StatusCode::OK),
    ] {
        assert_eq!(
            saved.access(method, "fixture-access").await.status(),
            status
        );
        for reader in &mut readers[..3] {
            reader.changed().await;
        }
        for reader in &mut readers {
            reader.quiet().await;
        }
    }
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), access_receipt(), access_receipt()],
    );
    assert_receipts(
        &saved.receipts_for(&dependent).await,
        &[access_receipt(), access_receipt()],
    );
    assert_receipts(
        &saved.receipts_for(&trigger).await,
        &[access_receipt(), access_receipt()],
    );
    assert_receipts(&saved.receipts_for(&unrelated).await, &[]);
    assert_receipts(&other.receipts().await, &[saved_receipt()]);
    other.case.cleanup().await;
    saved.case.cleanup().await;
}

async fn access_readers(saved: &Saved, other: &Saved) -> ([Uuid7; 3], [Reader; 5]) {
    let dependent = saved
        .neighbor("Support triage", json!({"credentials":[SERVICE]}))
        .await;
    let trigger = saved
        .neighbor(
            "Delivery review",
            json!({"credentials":[],"triggers":[
                {"type":"webhook", "source":"github", "credential_name":SERVICE}
            ]}),
        )
        .await;
    let unrelated = saved
        .neighbor(
            "Pull request review",
            json!({"credentials":[],"triggers":[]}),
        )
        .await;
    let readers = [
        Reader::open(saved, &saved.fleet).await,
        Reader::open(saved, &dependent).await,
        Reader::open(saved, &trigger).await,
        Reader::open(saved, &unrelated).await,
        Reader::open(other, &other.fleet).await,
    ];
    ([dependent, trigger, unrelated], readers)
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn memory_changes_notify_the_open_fleet_once_and_replays_stay_quiet() {
    let saved = Saved::create(json!({})).await;
    let neighbor = saved.neighbor("Support triage", json!({})).await;
    let other = Saved::create(json!({})).await;
    let mut own = Reader::open(&saved, &saved.fleet).await;
    let mut unrelated = Reader::open(&saved, &neighbor).await;
    let mut foreign = Reader::open(&other, &other.fleet).await;
    assert_eq!(saved.memory(true, false).await.status(), StatusCode::OK);
    own.changed().await;
    assert_eq!(saved.grants().await, json!({"read":true,"publish":false}));
    assert_receipts(
        &saved.receipts().await,
        &[
            saved_receipt(),
            ready_receipt(),
            memory_receipt(true, false),
        ],
    );
    assert_eq!(saved.memory(true, false).await.status(), StatusCode::OK);
    for reader in [&mut own, &mut unrelated, &mut foreign] {
        reader.quiet().await;
    }
    assert_receipts(&other.receipts().await, &[saved_receipt(), ready_receipt()]);
    other.case.cleanup().await;
    saved.case.cleanup().await;
}
