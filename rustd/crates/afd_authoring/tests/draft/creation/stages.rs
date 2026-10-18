//! A fault inside one setup stage leaves the retained fleet where that stage
//! stopped, and the same Save key finishes it without a second initial run.
//!
//! The earlier stages already have their own proofs: a stale review allocates
//! nothing, a missing wallet or a refused stream holds the fleet `installing`
//! (`identity.rs`), and a pending grant names its check (`grants.rs`). These
//! inject the faults those cannot reach: the activation transaction aborting
//! after every check passed, and the receipt of the committed initial run
//! failing to save.

use super::super::{fixture::now, mint, refusal::Refusal};
use super::fixture::{Case, NO_ADMISSIONS, NO_EVENTS, check};
use afd_authoring::{
    SavedFleet,
    types::{DraftState, SaveFleet, Start, StartOutcome, StartState},
};
use afd_core::id::Uuid7;
use afd_dragonfly::{FleetStreams, fleet_stream_key};
use sqlx::{Connection as _, PgConnection};

/// The initial run each case asks for, so a repeated stage would show as a
/// second admission.
const RUN: &str = "Run once setup finishes";
/// How long an occupying value outlives a failed test that never removes it.
const OCCUPIED_SECONDS: i64 = 300;

fn run_now() -> Start {
    Start::RunNow {
        message: RUN.to_owned(),
    }
}

/// The accepted event a finished Save reports, after asserting it is one.
fn accepted(saved: &SavedFleet) -> &str {
    assert_eq!(saved.status, "active");
    match &saved.start {
        StartOutcome::RunNow {
            state: StartState::Accepted,
            event_id: Some(event),
            error_code: None,
        } => event,
        other => unreachable_outcome(other),
    }
}

#[expect(clippy::panic, reason = "a start of the wrong shape fails the proof")]
fn unreachable_outcome(outcome: &StartOutcome) -> ! {
    panic!("unexpected initial run outcome: {outcome:?}")
}

/// The pending event an active Save reports, after asserting it is one.
fn pending_event(saved: &SavedFleet) -> String {
    assert_eq!(saved.status, "active");
    match &saved.start {
        StartOutcome::RunNow {
            state: StartState::Pending,
            event_id: Some(event),
            error_code: None,
        } => event.clone(),
        other => unreachable_outcome(other),
    }
}

/// Saves `request` unfunded, so setup stops at the wallet before any stream
/// or activation work, and answers the retained fleet.
async fn retained(c: &Case, key: &Uuid7, request: SaveFleet) -> SavedFleet {
    let first = c.save(key, request).await;
    assert_eq!(first.status, "installing");
    assert_eq!(check(&first, "wallet"), "pending");
    assert_eq!(check(&first, "stream"), "pending");
    first
}

/// Retries finish the fleet once: one admission, one stream event, and a
/// repeated retry neither admits nor appends again.
async fn assert_finishes_once(c: &Case, key: &Uuid7, request: SaveFleet, fleet: &Uuid7) {
    let finished = c.save(key, request.clone()).await;
    assert_eq!(&finished.id, fleet);
    let event = accepted(&finished).to_owned();
    let rows = c.admissions(fleet).await;
    assert_eq!(rows.len(), 1, "one initial admission: {rows:?}");
    assert_eq!(rows.first().map(|row| row.0.as_str()), Some(event.as_str()));
    assert_eq!(c.stream_events(fleet).await, std::slice::from_ref(&event));
    let again = c.save(key, request).await;
    assert_eq!(accepted(&again), event);
    assert_eq!(c.admissions(fleet).await, rows);
    assert_eq!(c.stream_events(fleet).await, [event]);
    assert_eq!(c.fleet_count().await, 1);
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_aborted_activation_rolls_back_its_run_and_the_same_key_finishes_it_once() {
    let c = Case::create().await;
    let (draft, request) = c.request("activation-fault", run_now()).await;
    let key = mint();
    let fleet = retained(&c, &key, request.clone()).await.id;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "fleet.fleets",
        "activate",
        fleet.as_str(),
        &format!("NEW.id='{fleet}'::uuid AND NEW.status='active'"),
    )
    .await;
    c.fund().await;
    c.creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("the activation transaction aborted");
    let stalled = c
        .creations
        .read(c.base.access(), &fleet)
        .await
        .expect("retained fleet");
    assert_eq!(stalled.status, "installing");
    assert_eq!(
        check(&stalled, "stream"),
        "ready",
        "the stage before stays done"
    );
    assert!(
        matches!(stalled.start, StartOutcome::RunNow { event_id: None, .. }),
        "the staged run rolled back with the activation"
    );
    assert_eq!(c.admissions(&fleet).await, NO_ADMISSIONS);
    assert_eq!(c.stream_events(&fleet).await, NO_EVENTS);
    let saving = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(
        saving.expect("draft stays linked").state,
        DraftState::Saving
    );
    drop(refusal);
    assert_finishes_once(&c, &key, request, &fleet).await;
    let closed = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(closed.expect("closed draft").state, DraftState::Closed);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_unsaved_receipt_answers_the_save_pending_and_the_same_key_accepts_that_event() {
    let c = Case::create().await;
    let (_, request) = c.request("receipt-fault", run_now()).await;
    let key = mint();
    let fleet = retained(&c, &key, request.clone()).await.id;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "fleet.admissions",
        "receipt",
        fleet.as_str(),
        &format!("NEW.fleet_id='{fleet}'::uuid AND NEW.receipt IS NOT NULL"),
    )
    .await;
    c.fund().await;
    let event = pending_event(&c.save(&key, request.clone()).await);
    let unreceipted = c.admissions(&fleet).await;
    assert_eq!(unreceipted, [(event.clone(), None)]);
    drop(refusal);
    let finished = c.save(&key, request.clone()).await;
    assert_eq!(accepted(&finished), event);
    let rows = c.admissions(&fleet).await;
    assert_eq!(rows.len(), 1, "one initial admission: {rows:?}");
    assert!(
        rows.iter()
            .all(|(id, receipt)| *id == event && receipt.is_some())
    );
    let appended = c.stream_events(&fleet).await;
    assert!(
        appended.iter().all(|id| *id == event),
        "one logical event: {appended:?}"
    );
    assert_eq!(accepted(&c.save(&key, request).await), event);
    assert_eq!(c.admissions(&fleet).await, rows);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_refused_append_or_a_held_row_answers_pending_and_the_same_key_accepts_that_event() {
    let c = Case::create().await;
    let (_, request) = c.request("append-fault", run_now()).await;
    let key = mint();
    let fleet = retained(&c, &key, request.clone()).await.id;
    refuse_appends(&c, &fleet).await;
    c.fund().await;
    let event = pending_event(&c.save(&key, request.clone()).await);
    assert_eq!(c.admissions(&fleet).await, [(event.clone(), None)]);
    FleetStreams::new(c.queue.clone())
        .forget(fleet.as_str())
        .await
        .expect("the queue recovers");
    let mut sibling = sibling_repair(&c, &fleet).await;
    let held = c.save(&key, request.clone()).await;
    assert_eq!(pending_event(&held), event, "a held row stays pending");
    sqlx::query("COMMIT")
        .execute(&mut sibling)
        .await
        .expect("the sibling finishes");
    assert_eq!(accepted(&c.save(&key, request).await), event);
    assert_eq!(c.stream_events(&fleet).await, std::slice::from_ref(&event));
    let rows = c.admissions(&fleet).await;
    assert!(
        rows.iter()
            .all(|(id, receipt)| *id == event && receipt.is_some())
    );
    assert_eq!(rows.len(), 1);
    c.cleanup().await;
}

/// The stream stage finished, then the fleet's stream key turned into a plain
/// value: activation commits, and its append meets WRONGTYPE.
async fn refuse_appends(c: &Case, fleet: &Uuid7) {
    sqlx::query("UPDATE fleet.fleets SET setup_stream_ready=true WHERE id=$1::uuid")
        .bind(fleet.as_str())
        .execute(&mut *c.base.connection().await)
        .await
        .expect("the stream stage finished");
    c.queue
        .set_for(
            &fleet_stream_key(fleet.as_str()),
            "occupied",
            OCCUPIED_SECONDS,
        )
        .await
        .expect("occupy the stream key");
}

/// A sibling repair holding the fleet's admission row, as the replay sweep
/// does while it confirms that row.
async fn sibling_repair(c: &Case, fleet: &Uuid7) -> PgConnection {
    let mut sibling = PgConnection::connect(&c.base.owner_url())
        .await
        .expect("sibling connects");
    for step in [
        "BEGIN",
        "SELECT id FROM fleet.admissions WHERE fleet_id=$1::uuid FOR UPDATE",
    ] {
        sqlx::query(step)
            .bind(fleet.as_str())
            .execute(&mut sibling)
            .await
            .expect("hold the admission row");
    }
    sibling
}
