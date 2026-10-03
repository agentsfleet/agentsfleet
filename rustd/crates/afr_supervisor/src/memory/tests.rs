#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::id::Uuid7;
use afd_wire::memory::MemoryHydrateResponse;

use afd_core::test_util::trace::Capture;
use afr_agent::Checkpoint as _;

use super::{LeaseCheckpoint, capture, hydrate};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, FLEET_ID, LEASE_ID, answer, drain, json, lease, plane};

#[tokio::test(start_paused = true)]
async fn hydrate_rides_out_a_blip() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (plane, _calls) = plane(move |_call| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Answer::Fail(error::unavailable(Verb::Hydrate, 503))
        } else {
            json(&MemoryHydrateResponse {
                memory: answer().memory,
            })
        }
    });

    let body = hydrate(&plane, &Uuid7::parse(FLEET_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(
        body.decode::<MemoryHydrateResponse<'_>>()
            .unwrap()
            .memory
            .len(),
        1
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn capture_carries_the_lease_and_its_fencing_token() {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"stored": 1, "skipped": 0})));
    let lease = lease(LEASE_ID, FLEET_ID, None);

    capture(
        &plane,
        &Uuid7::parse(FLEET_ID).unwrap(),
        &lease,
        answer().memory,
    )
    .await
    .unwrap();

    let pushed = drain(&mut calls).remove(0);
    let body: serde_json::Value = serde_json::from_slice(&pushed.body.unwrap()).unwrap();
    assert_eq!(body["lease_id"], LEASE_ID);
    assert_eq!(body["fencing_token"], FENCING);
    assert_eq!(body["memory"][0]["key"], "k");
}

#[tokio::test]
async fn a_checkpoint_pushes_the_runs_memory_under_the_fencing_token() {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"stored": 1, "skipped": 0})));
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();

    LeaseCheckpoint::new(&plane, &fleet, &lease)
        .push(answer().memory)
        .await;

    let pushed = drain(&mut calls);
    assert_eq!(pushed.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(pushed[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["fencing_token"], FENCING);
    assert_eq!(body["memory"][0]["key"], "k");
}

#[tokio::test(start_paused = true)]
async fn a_checkpoint_the_daemon_refuses_is_logged_once_and_never_retried() {
    let capture = Capture::install();
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (plane, _calls) = plane(move |_call| {
        counted.fetch_add(1, Ordering::SeqCst);
        Answer::Fail(error::unavailable(Verb::Capture, 503))
    });
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();

    LeaseCheckpoint::new(&plane, &fleet, &lease)
        .push(answer().memory)
        .await;

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a checkpoint is one attempt"
    );
    let failed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("memory_checkpoint_failed"))
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].field("lease_id"), Some(LEASE_ID));
}
