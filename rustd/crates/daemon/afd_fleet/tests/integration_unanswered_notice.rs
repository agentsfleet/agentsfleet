//! A thread that asked is told when its event ends with nothing to say.
//!
//! Three endings write no answer: a run that failed before it wrote any text,
//! an event a gate refused, and an event ended after its leases kept lapsing.
//! Each owes the admission's thread one fixed notice through the delivery
//! ledger. What must not change is proven beside it: a clean run that wrote
//! nothing still owes nothing, an event asked from nowhere owes nothing
//! however it ends, and the event row never carries the notice as though the
//! fleet had said it.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without datastores, and `make test-integration-rustd` is the only lane that
//! executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::event::label::{BALANCE_EXHAUSTED, LEASE_EXPIRED};
use af_core::event::status::FLEET_ERROR;
use af_wire::report::FailureClass;
use afd_admission::{Producer, Reply};
use afd_fleet::lease::admit::Refusal;
use afd_fleet::lease::event::DETAIL_LEASES_LAPSED;
use afd_fleet::lease::{Acquired, Committed, Ended, Verdict};
use futures_util::future::join_all;
use tokio::sync::Barrier;

use crate::integration_report_owes_destination::stage::{Stage, obligations, owed_by, stage};
use crate::report_commit::report;
use crate::report_seed::SLICE_MS;
use crate::rows::Row;

/// A run the runner killed at its deadline, before it wrote any answer.
const TIMED_OUT: Verdict<'static> = Verdict::Failed {
    class: Some(FailureClass::TimeoutKill),
    detail: "the run outran its deadline",
};

/// How many gate passes race to refuse one event.
const REFUSERS: usize = 100;

/// The answers the ledger owes this fleet, oldest first.
async fn answers(stage: &Stage) -> Vec<String> {
    obligations(&stage.fixtures, &stage.fleet)
        .await
        .into_iter()
        .map(|owed| owed.answer)
        .collect()
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

/// The fleet's next event, claimed and open, with no run reporting on it.
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
        answers(&stage).await,
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

    assert_eq!(answers(&stage).await, vec!["the half I finished"]);

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_clean_run_that_wrote_nothing_still_owes_nothing() {
    let stage = stage().await;
    stage.admit_thread("clean-and-silent").await;

    let committed = run_ending(&stage, Verdict::Succeeded, "").await;

    assert_eq!(owed_by(committed), None);
    assert_eq!(answers(&stage).await, Vec::<String>::new());

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_run_asked_from_nowhere_owes_nothing_however_it_ends() {
    for (verdict, answer) in [
        (TIMED_OUT, ""),
        (TIMED_OUT, "the half I finished"),
        (Verdict::Succeeded, ""),
    ] {
        let stage = stage().await;
        stage
            .admit(Producer::ScheduleFire, "asked-from-nowhere", Reply::None)
            .await;

        let committed = run_ending(&stage, verdict, answer).await;

        assert_eq!(owed_by(committed), None, "{verdict:?} saying {answer:?}");
        assert_eq!(answers(&stage).await, Vec::<String>::new());

        stage.fixtures.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_refused_by_racing_passes_owes_its_thread_one_refusal() {
    let stage = stage().await;
    stage.admit_thread("refused-in-a-thread").await;
    let acquired = claimed(&stage).await;
    let barrier = Barrier::new(REFUSERS);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);

    let endings = join_all((0..REFUSERS).map(|_| {
        let (leases, acquired) = (&stage.leases, &acquired);
        let (barrier, active, peak) = (&barrier, &active, &peak);
        async move {
            barrier.wait().await;
            let in_flight = active.fetch_add(1, Ordering::Relaxed) + 1;
            peak.fetch_max(in_flight, Ordering::Relaxed);
            let ending = leases
                .block(acquired, Refusal::labelled(BALANCE_EXHAUSTED))
                .await;
            active.fetch_sub(1, Ordering::Relaxed);
            ending
        }
    }))
    .await;

    assert!(peak.load(Ordering::Relaxed) > 1, "the passes overlapped");
    let ended = endings
        .iter()
        .filter(|ending| matches!(ending, Ok(Ended::Now(_))))
        .count();
    let fenced = endings
        .iter()
        .filter(|ending| {
            matches!(ending, Err(refused)
                if refused.code() == af_core::error_code::RUN_STALE_FENCING_TOKEN)
        })
        .count();
    assert_eq!(
        (ended, fenced),
        (1, REFUSERS - 1),
        "one pass ends the event, and every other finds its admission settled"
    );
    assert_eq!(
        answers(&stage).await,
        vec![format!(
            "This request was not run (balance_exhausted). A workspace member can read why with \
             `agentsfleet events {}`.",
            stage.fleet
        )]
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
    assert_eq!(answers(&stage).await, Vec::<String>::new());

    stage.fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_ended_after_its_leases_lapsed_owes_its_thread_the_notice() {
    let stage = stage().await;
    let event = stage.admit_thread("lapsed-in-a-thread").await;
    let acquired = claimed(&stage).await;

    let ended = stage
        .leases
        .abandon(&acquired)
        .await
        .expect("the ending must write");

    assert!(matches!(ended, Ended::Now(_)));
    assert_eq!(
        answers(&stage).await,
        vec![format!(
            "This run ended before it could answer (lease_expired). A workspace member can read \
             what happened with `agentsfleet events {}`.",
            stage.fleet
        )]
    );
    let stored = Row::Event(&stage.fleet, &event);
    assert_eq!(
        (
            stage.fixtures.text(stored, "status").await,
            stage.fixtures.text(stored, "failure_reason").await,
            stage.fixtures.text(stored, "failure_detail").await,
        ),
        (
            Some(FLEET_ERROR.to_owned()),
            Some(LEASE_EXPIRED.to_owned()),
            Some(DETAIL_LEASES_LAPSED.to_owned()),
        ),
        "the row says a run failed, and why"
    );

    stage.fixtures.cleanup().await;
}
