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
use super::fixture::{Case, NO_ADMISSIONS, check};
use afd_admission::{Admissions, Budgets};
use afd_authoring::Creations;
use afd_authoring::{
    SavedFleet,
    types::{DraftState, SaveFleet, Start, StartOutcome, StartState},
};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;

/// The initial run each case asks for, so a repeated stage would show as a
/// second admission.
const RUN: &str = "Run once setup finishes";
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
    assert_eq!(rows.first().map(String::as_str), Some(event.as_str()));
    let again = c.save(key, request).await;
    assert_eq!(accepted(&again), event);
    assert_eq!(c.admissions(fleet).await, rows);
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
        "RAISE EXCEPTION 'fixture refusal'",
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
async fn an_initial_run_over_the_fleet_backlog_budget_activates_blocked() {
    let c = Case::create().await;
    c.fund().await;
    let full =
        Admissions::new(c.base.db.clone(), c.queue.clone(), Entropy::new()).with_budgets(Budgets {
            fleet_backlog: 0,
            ..Budgets::default()
        });
    let creations = Creations::new(c.setup.clone(), full);
    let start = Start::RunNow {
        message: "Run once setup finishes".to_owned(),
    };
    let (_, request) = c.request("backlog-full", start).await;
    let saved = creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect("a full backlog holds the run, not the setup");
    assert_eq!(saved.status, "active");
    let blocked = matches!(
        &saved.start,
        StartOutcome::RunNow {
            state: StartState::Blocked,
            event_id: None,
            error_code: Some(code),
        } if code == error_code::INTERNAL_DB_UNAVAILABLE.as_str()
    );
    assert!(
        blocked,
        "the run names the capacity refusal: {:?}",
        saved.start
    );
    assert_eq!(c.admissions(&saved.id).await, NO_ADMISSIONS);
    c.cleanup().await;
}
