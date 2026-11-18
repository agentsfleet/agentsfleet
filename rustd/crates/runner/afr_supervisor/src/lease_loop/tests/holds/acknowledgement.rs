//! Successful settlement need not retain the runner's parked sandbox.

#![expect(clippy::unwrap_used, reason = "a failed fixture fails the test")]

use super::tests::{holding, settled};
use crate::client::Verb;
use crate::test_support::{Behaviour, FLEET_ID, FakeEngine, LEASE_ID, json, lease};

#[tokio::test(start_paused = true)]
async fn a_successful_report_without_an_accepted_hold_discards_the_exact_park() {
    let (rig, counted) = holding(
        |call| (call.verb == Verb::Report).then(|| json(&serde_json::json!({"ok":true}))),
        FakeEngine::default(),
        Behaviour::Answer,
    );
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    assert_eq!(
        rig.lessee.holds.fleets().await,
        Vec::<af_core::id::Uuid7>::new()
    );
    settled(&rig).await;
    assert_eq!(counted.read().3, 1, "the declined hold is torn down");
}

#[tokio::test(start_paused = true)]
async fn a_hold_receipt_for_a_different_attempt_cannot_keep_the_park() {
    let (rig, counted) = holding(
        |call| {
            (call.verb == Verb::Report).then(|| {
                json(&serde_json::json!({
                    "ok":true, "hold":{"lease_id":FLEET_ID,"fencing_token":999,
                        "expires_at":"2099-01-01T00:00:00Z"}
                }))
            })
        },
        FakeEngine::default(),
        Behaviour::Answer,
    );
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    assert_eq!(
        rig.lessee.holds.fleets().await,
        Vec::<af_core::id::Uuid7>::new()
    );
    settled(&rig).await;
    assert_eq!(counted.read().3, 1);
}

#[tokio::test(start_paused = true)]
async fn a_matching_receipt_cannot_extend_the_requested_hold() {
    let (rig, counted) = holding(
        |call| {
            (call.verb == Verb::Report).then(|| {
                let body: serde_json::Value =
                    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
                json(&serde_json::json!({"ok": true, "hold": {
                    "lease_id": body.get("lease_id").unwrap(),
                    "fencing_token": body.get("fencing_token").unwrap(),
                    "expires_at": "2099-01-01T00:00:00Z"
                }}))
            })
        },
        FakeEngine::default(),
        Behaviour::Answer,
    );
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    assert_eq!(
        rig.lessee.holds.fleets().await,
        Vec::<af_core::id::Uuid7>::new()
    );
    settled(&rig).await;
    assert_eq!(counted.read().3, 1);
}
