//! A thread that asked is told when its event ends with nothing to say.
//!
//! Two endings write no answer: a run that failed before it wrote any text,
//! and an event a gate refused. Each owes the admission's thread one fixed
//! notice through the delivery ledger. What must not change is proven beside
//! it: a clean run that wrote nothing still owes nothing, an event asked from
//! nowhere owes nothing either way, and the event row never carries the notice
//! as though the fleet had said it.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without datastores, and `make test-integration-rustd` is the only lane that
//! executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::event::label::{APPROVAL_DENIED, BALANCE_EXHAUSTED};
use af_wire::report::FailureClass;
use afd_admission::{Producer, Reply};
use afd_fleet::lease::admit::Refusal;
use afd_fleet::lease::{Acquired, Committed, Ended, Verdict};

use crate::integration_report_owes_destination::stage::{Stage, owed_by, stage};
use crate::report_commit::report;
use crate::report_seed::SLICE_MS;
use crate::rows::Row;

/// A run the runner killed at its deadline, before it wrote any answer.
const TIMED_OUT: Verdict<'static> = Verdict::Failed {
    class: Some(FailureClass::TimeoutKill),
    detail: "the run outran its deadline",
};

/// The answers the ledger owes this fleet, oldest first.
async fn owed_answers(stage: &Stage) -> Vec<String> {
    let mut connection = stage
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar(
        "SELECT answer FROM fleet.obligations WHERE fleet_id = $1 ORDER BY created_at, sequence",
    )
    .bind(&stage.fleet)
    .fetch_all(&mut *connection)
    .await
    .expect("the ledger answers")
}

/// Leases the fleet's next event and reports it ended `verdict`, saying `answer`.
async fn run_ending(stage: &Stage, verdict: Verdict<'_>, answer: &str) -> Committed {
    let issued_at = stage.fixtures.enrolled_at();
    let (lease_id, lease) = stage.lease_next(issued_at).await;
    let pricing = stage.fixtures.pricing(&lease_id).await;
    let reported_at = af_core::clock::after(
        issued_at,
        std::time::Duration::from_millis(
            u64::try_from(SLICE_MS).expect("a fixture span is not negative"),
        ),
    );
    let mut ending = report(
        &lease_id,
        &stage.runner,
        &lease,
        (&lease_id, &pricing),
        answer,
        reported_at,
    );
    ending.outcome.verdict = verdict;
    stage
        .leases
        .commit_report(ending)
        .await
        .expect("the report must reach the datastore")
}

/// The fleet's next event, claimed and open, ready for a gate to refuse.
async fn claimed(stage: &Stage) -> Acquired {
    crate::seed::select_fleet(&stage.leases, &stage.runner, &stage.fleet)
        .await
        .expect("the fleet is leasable")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_run_that_wrote_nothing_owes_its_thread_the_notice() {
    let stage = stage().await;
    let event = stage.admit_thread("failed-in-a-thread").await;

    let committed = run_ending(&stage, TIMED_OUT, "").await;

    assert!(
        owed_by(committed).is_some(),
        "the report names the row it owed, so the outbound workers are woken for it"
    );
    assert_eq!(
        owed_answers(&stage).await,
        vec![format!(
            "This run ended before it could answer (timeout_kill). A workspace member can read \
             what happened with `agentsfleet events {}`.",
            stage.fleet
        )]
    );
    assert_eq!(
        stage
            .fixtures
            .text(Row::Event(&stage.fleet, &event), "response_text")
            .await,
        Some(String::new()),
        "the event keeps what the run said, which was nothing"
    );

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_run_that_wrote_an_answer_owes_that_answer_and_no_notice() {
    let stage = stage().await;
    stage.admit_thread("failed-after-answering").await;

    run_ending(&stage, TIMED_OUT, "the half I finished").await;

    assert_eq!(owed_answers(&stage).await, vec!["the half I finished"]);

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_clean_run_that_wrote_nothing_still_owes_nothing() {
    let stage = stage().await;
    stage.admit_thread("clean-and-silent").await;

    let committed = run_ending(&stage, Verdict::Succeeded, "").await;

    assert_eq!(owed_by(committed), None);
    assert_eq!(owed_answers(&stage).await, Vec::<String>::new());

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_run_asked_from_nowhere_owes_nothing() {
    let stage = stage().await;
    stage
        .admit(Producer::ScheduleFire, "failed-on-a-schedule", Reply::None)
        .await;

    let committed = run_ending(&stage, TIMED_OUT, "").await;

    assert_eq!(owed_by(committed), None);
    assert_eq!(owed_answers(&stage).await, Vec::<String>::new());

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_refused_at_a_gate_owes_its_thread_the_refusal_once() {
    let stage = stage().await;
    stage.admit_thread("refused-in-a-thread").await;
    let acquired = claimed(&stage).await;

    let first = stage
        .leases
        .block(&acquired, Refusal::labelled(BALANCE_EXHAUSTED))
        .await
        .expect("the refusal must write");
    let second = stage
        .leases
        .block(&acquired, Refusal::labelled(APPROVAL_DENIED))
        .await
        .expect_err("a settled admission refuses another mutation");

    assert!(matches!(first, Ended::Now(_)));
    assert_eq!(second.code(), af_core::error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(
        owed_answers(&stage).await,
        vec![format!(
            "This request was not run (balance_exhausted). A workspace member can read why with \
             `agentsfleet events {}`.",
            stage.fleet
        )],
        "the pass that ended the event owed its thread, and the refused repeat owed nothing"
    );

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_refused_at_a_gate_and_asked_from_nowhere_owes_nothing() {
    let stage = stage().await;
    stage
        .admit(Producer::Webhook, "refused-from-a-webhook", Reply::None)
        .await;
    let acquired = claimed(&stage).await;

    let ended = stage
        .leases
        .block(&acquired, Refusal::labelled(BALANCE_EXHAUSTED))
        .await
        .expect("the refusal must write");

    assert!(matches!(ended, Ended::Now(_)));
    assert_eq!(owed_answers(&stage).await, Vec::<String>::new());

    stage.fixtures.cleanup().await;
}
