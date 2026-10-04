#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::id::Uuid7;
use afd_wire::memory::MemoryHydrateResponse;

use afr_agent::Checkpoint as _;
use afr_memory::Recall as _;

use super::{LeaseCheckpoint, Recaller, capture, hydrate};
use crate::client::Verb;
use crate::error;
use crate::test_support::{
    Answer, FENCING, FLEET_ID, LEASE_ID, answer, daemon, drain, json, lease, plane,
};

/// What a model searched for past its window.
const QUERY: &str = "deploy";
/// How many entries that search asked for.
const LIMIT: usize = 3;

/// Asserts a refusal carries the daemon's code rather than one of its own.
#[track_caller]
fn assert_kept<C: PartialEq + std::fmt::Debug>(got: &C, daemons: &C) {
    assert_eq!(got, daemons, "the daemon's code, kept");
}

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
                shared: Vec::new(),
                publish: false,
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
        .await
        .unwrap();

    let pushed = drain(&mut calls);
    assert_eq!(pushed.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(pushed[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["fencing_token"], FENCING);
    assert_eq!(body["memory"][0]["key"], "k");
}

#[tokio::test(start_paused = true)]
async fn a_checkpoint_the_daemon_refuses_is_returned_once_and_never_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (plane, _calls) = plane(move |_call| {
        counted.fetch_add(1, Ordering::SeqCst);
        Answer::Fail(error::unavailable(Verb::Capture, 503))
    });
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();

    let refused = LeaseCheckpoint::new(&plane, &fleet, &lease)
        .push(answer().memory)
        .await
        .unwrap_err();

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a checkpoint is one attempt"
    );
    let daemons = error::unavailable(Verb::Capture, 503).code();
    assert_kept(&refused.code(), &daemons);
}

#[tokio::test]
async fn a_recall_carries_the_query_under_the_fencing_token() {
    let (plane, mut calls) = plane(daemon(|_call| None));
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();

    let found = Recaller::new(&plane, &fleet, &lease)
        .recall(QUERY, LIMIT)
        .await
        .unwrap();

    assert!(found.memory.is_empty() && found.shared.is_empty());
    let asked = drain(&mut calls);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].verb, Verb::Recall);
    let body: serde_json::Value = serde_json::from_slice(asked[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["lease_id"], LEASE_ID);
    assert_eq!(body["fencing_token"], FENCING);
    assert_eq!(body["query"], QUERY);
    assert_eq!(body["limit"], LIMIT);
}

#[tokio::test(start_paused = true)]
async fn a_recall_the_daemon_refuses_is_unanswered_under_its_code_and_never_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (plane, _calls) = plane(move |_call| {
        counted.fetch_add(1, Ordering::SeqCst);
        Answer::Fail(error::unavailable(Verb::Recall, 503))
    });
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();

    let unanswered = Recaller::new(&plane, &fleet, &lease)
        .recall(QUERY, LIMIT)
        .await
        .unwrap_err();

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a recall is one attempt"
    );
    let daemons = error::unavailable(Verb::Recall, 503).code();
    assert_kept(&unanswered.code(), &daemons);
}
