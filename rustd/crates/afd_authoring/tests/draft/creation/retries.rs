//! A crash between two setup steps leaves only what had committed, and every
//! retry, one after another or all at once, converges on one fleet, one
//! revision, one stream group and one initial run.
//!
//! `stages.rs` covers the steps from activation on. These cover the boundaries
//! before it and the concurrent retry: the reservation aborting after its fleet
//! row was written, the stream group created while the row recording it fails
//! to save, and many retries racing one stalled setup.

use super::super::{fixture::now, mint, refusal::Refusal};
use super::fixture::{Case, NO_ADMISSIONS, NO_EVENTS, check};
use afd_authoring::{
    SavedFleet,
    types::{DraftState, SaveFleet, Start, StartOutcome, StartState},
};
use afd_core::{error_code, id::Uuid7};
use afd_dragonfly::streams::FLEET_CONSUMER_GROUP;
use std::sync::Arc;
use tokio::sync::Barrier;

/// The initial run each case asks for, so a repeated stage shows as a second
/// admission.
const RUN: &str = "Run once setup finishes";
/// How many identical retries race one stalled setup.
const RETRIES: usize = 8;

fn run_now() -> Start {
    Start::RunNow {
        message: RUN.to_owned(),
    }
}

/// The event an initial run names, whether its receipt is confirmed yet or not.
fn named_event(saved: &SavedFleet) -> Option<&str> {
    match &saved.start {
        StartOutcome::RunNow {
            state: StartState::Accepted | StartState::Pending,
            event_id: Some(event),
            error_code: None,
        } => Some(event),
        _ => None,
    }
}

/// Saves `request` unfunded, so setup stops at the wallet before any stream
/// or activation work, and answers the retained fleet.
async fn retained(c: &Case, key: &Uuid7, request: SaveFleet) -> Uuid7 {
    let first = c.save(key, request).await;
    assert_eq!(first.status, "installing");
    assert_eq!(check(&first, "stream"), "pending");
    first.id
}

/// Receipts written under `key`: a spent Save key has exactly one.
async fn receipts(c: &Case, key: &Uuid7) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM core.authoring_replay_receipts WHERE workspace_id=$1::uuid AND operation_id=$2::uuid",
    )
    .bind(c.base.workspace.as_str())
    .bind(key.as_str())
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("replay receipts")
}

async fn revisions(c: &Case, fleet: &Uuid7) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM core.fleet_definition_revisions WHERE fleet_id=$1::uuid",
    )
    .bind(fleet.as_str())
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("definition revisions")
}

async fn groups(c: &Case, fleet: &Uuid7) -> Vec<String> {
    afd_dragonfly::test_util::fleet_groups(&c.queue, fleet.as_str())
        .await
        .expect("the fleet stream exists")
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_reservation_aborted_after_its_fleet_row_leaves_nothing_and_the_key_creates_one_fleet() {
    let c = Case::create().await;
    let (draft, request) = c.request("reservation-fault", run_now()).await;
    let key = mint();
    // The reservation inserts the fleet row, then points it at its first
    // revision; refusing that pointer aborts the transaction with the row
    // already written. Keyed by the fixture's own workspace, because the fleet
    // ID is minted inside the transaction under test.
    let workspace = &c.base.workspace;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "core.fleets",
        "revision",
        workspace.as_str(),
        &format!(
            "NEW.workspace_id='{workspace}'::uuid AND OLD.current_definition_revision_id IS NULL"
        ),
        "RAISE EXCEPTION 'fixture refusal'",
    )
    .await;
    let aborted = c
        .creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("the reservation aborted after its fleet row");
    assert_eq!(aborted.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(c.fleet_count().await, 0, "the fleet row rolled back");
    assert_eq!(receipts(&c, &key).await, 0, "the key was not spent");
    let unlinked = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(unlinked.expect("draft").state, DraftState::Editing);
    drop(refusal);
    let first = c.save(&key, request.clone()).await;
    assert_eq!(first.status, "installing");
    assert_eq!(c.save(&key, request).await.id, first.id);
    assert_eq!(c.fleet_count().await, 1);
    assert_eq!(receipts(&c, &key).await, 1);
    assert_eq!(revisions(&c, &first.id).await, 1);
    assert_eq!(c.admissions(&first.id).await, NO_ADMISSIONS);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_stream_group_created_while_its_record_failed_is_reused_by_the_retry() {
    let c = Case::create().await;
    let (_, request) = c.request("stream-record-fault", run_now()).await;
    let key = mint();
    let fleet = retained(&c, &key, request.clone()).await;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "core.fleets",
        "stream_ready",
        fleet.as_str(),
        &format!(
            "NEW.id='{fleet}'::uuid AND NEW.setup_stream_ready AND NOT OLD.setup_stream_ready"
        ),
        "RAISE EXCEPTION 'fixture refusal'",
    )
    .await;
    c.fund().await;
    let lost = c
        .creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("the stream record failed to save");
    assert_eq!(lost.code(), error_code::INTERNAL_DB_QUERY);
    let stalled = c
        .creations
        .read(c.base.access(), &fleet)
        .await
        .expect("retained fleet");
    assert_eq!(stalled.status, "installing");
    assert_eq!(check(&stalled, "stream"), "pending", "the record was lost");
    assert_eq!(groups(&c, &fleet).await, [FLEET_CONSUMER_GROUP]);
    assert_eq!(c.admissions(&fleet).await, NO_ADMISSIONS);
    assert_eq!(c.stream_events(&fleet).await, NO_EVENTS);
    drop(refusal);
    let finished = c.save(&key, request.clone()).await;
    assert_eq!(finished.id, fleet);
    assert_eq!(finished.status, "active");
    let event = named_event(&finished).expect("one initial run").to_owned();
    assert_eq!(groups(&c, &fleet).await, [FLEET_CONSUMER_GROUP]);
    assert_eq!(c.admissions(&fleet).await.len(), 1);
    assert_eq!(c.stream_events(&fleet).await, std::slice::from_ref(&event));
    assert_eq!(
        named_event(&c.save(&key, request).await),
        Some(event.as_str())
    );
    assert_eq!(c.admissions(&fleet).await.len(), 1);
    c.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn concurrent_retries_after_an_aborted_activation_admit_one_run_on_one_stream() {
    let c = Arc::new(Case::create().await);
    let (_, request) = c.request("concurrent-retry", run_now()).await;
    let key = mint();
    let fleet = retained(&c, &key, request.clone()).await;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "core.fleets",
        "activate",
        fleet.as_str(),
        &format!("NEW.id='{fleet}'::uuid AND NEW.status='active'"),
        "RAISE EXCEPTION 'fixture refusal'",
    )
    .await;
    c.fund().await;
    c.creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("the activation transaction aborted");
    drop(refusal);
    let barrier = Arc::new(Barrier::new(RETRIES));
    let tasks: Vec<_> = (0..RETRIES)
        .map(|_| {
            let (c, request, key, barrier) = (
                Arc::clone(&c),
                request.clone(),
                key.clone(),
                Arc::clone(&barrier),
            );
            tokio::spawn(async move {
                barrier.wait().await;
                c.creations
                    .save(c.base.access(), &key, request, now())
                    .await
            })
        })
        .collect();
    let mut events = Vec::new();
    for task in tasks {
        let saved = task.await.expect("retry task").expect("a retry resumes");
        assert_eq!(saved.id, fleet);
        assert_eq!(saved.status, "active");
        events.push(named_event(&saved).expect("the initial run").to_owned());
    }
    let event = events.first().expect("retries ran").clone();
    assert!(events.iter().all(|seen| *seen == event), "{events:?}");
    let settled = c.save(&key, request).await;
    assert_eq!(named_event(&settled), Some(event.as_str()));
    let rows = c.admissions(&fleet).await;
    assert_eq!(rows.len(), 1, "one initial admission: {rows:?}");
    assert!(rows.iter().all(|(_, receipt)| receipt.is_some()));
    assert!(c.stream_events(&fleet).await.iter().all(|id| *id == event));
    assert_eq!(groups(&c, &fleet).await, [FLEET_CONSUMER_GROUP]);
    assert_eq!(c.fleet_count().await, 1);
    Arc::try_unwrap(c)
        .ok()
        .expect("all retry clones dropped")
        .cleanup()
        .await;
}
