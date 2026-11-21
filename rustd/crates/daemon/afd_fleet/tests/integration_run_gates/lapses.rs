//! An event whose runs keep lapsing is ended, and its fleet's queue moves.
//!
//! A lapsed run is reassigned and its event runs again from the start, which
//! is the recovery a dead runner needs. An event that outlives every runner it
//! is given would be re-held without end and hold the head of its fleet's
//! queue, so the pass counts the runs an event was issued and stops at the
//! cap.

use af_core::limits::EVENT_RUNS_MAX;
use af_core::test_util::trace::Capture;
use af_core::turn::label::RUN_EXPIRED;
use af_core::turn::status::FLEET_ERROR;
use afd_fleet::run::Billed;
use afd_fleet::run::turn::DETAIL_RUNS_LAPSED;

use super::seed::seed_provider_resolution;
use super::*;

use crate::rows::Row;
use crate::seed::{MODEL, POSTURE, PROVIDER};

/// One funded, leasable fleet and the chat event at the head of its queue.
async fn funded(fixtures: &Fixtures) -> (Ready, Uuid7) {
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(fixtures).await;
    set_config(fixtures, &fleet, BUDGETED_CONFIG).await;
    let turn_id = chat(fixtures, &fleet, &workspace).await;
    let seeded = Ready {
        runner,
        fleet,
        turn_id,
        tenant,
    };
    (seeded, workspace)
}

/// One more chat event on the fleet's queue.
async fn chat(fixtures: &Fixtures, fleet: &Uuid7, workspace: &Uuid7) -> Uuid7 {
    crate::queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await
}

/// Runs `seeded`'s event `times` over, each holder lapsing without a report.
async fn lapse(fixtures: &Fixtures, seeded: &Ready, times: u32) {
    let runs = fixtures.runs();
    for _holder in 0..times {
        let assigned = assign(fixtures, seeded).await;
        let issued = runs
            .issue(
                &seeded.runner,
                &assigned,
                Billed {
                    tenant_id: &seeded.tenant,
                    posture: POSTURE,
                    provider: PROVIDER,
                    model: MODEL,
                },
                fixtures.enrolled_at(),
            )
            .await
            .expect("the run row must be written")
            .expect("the assignment is still held");
        fixtures.lapse_holder(&issued.run_id, &seeded.fleet).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_whose_every_run_lapsed_is_ended_as_failed_and_frees_the_queue() {
    let fixtures = Fixtures::create().await;
    let (seeded, workspace) = funded(&fixtures).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    lapse(&fixtures, &seeded, EVENT_RUNS_MAX).await;

    let assigned = assign(&fixtures, &seeded).await;
    let counted = assigned.earlier_runs;
    let answer = drive(&fixtures, &seeded, assigned).await;

    assert_eq!(
        counted,
        i64::from(EVENT_RUNS_MAX),
        "the assignment counts every run the event was issued"
    );
    assert!(answer.is_none(), "a fourth run was issued: {answer:?}");
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await,
        Some((FLEET_ERROR.to_owned(), RUN_EXPIRED.to_owned()))
    );
    assert_eq!(
        fixtures
            .text(Row::Event(&seeded.fleet, &seeded.turn_id), "failure_detail")
            .await
            .as_deref(),
        Some(DETAIL_RUNS_LAPSED)
    );

    let next = chat(&fixtures, &seeded.fleet, &workspace).await;
    let held = assign(&fixtures, &seeded).await;
    assert_eq!(
        (held.turn_id, held.earlier_runs),
        (next, 0),
        "the fleet's next event is at the head of the queue, counted from nothing"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_one_lapse_short_of_the_cap_is_assigned_again() {
    let fixtures = Fixtures::create().await;
    let (seeded, _workspace) = funded(&fixtures).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    lapse(&fixtures, &seeded, EVENT_RUNS_MAX - 1).await;

    let assigned = assign(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, assigned).await;

    assert!(answer.is_some(), "the last run the cap allows was refused");
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await,
        Some((STATUS_RECEIVED.to_owned(), String::new())),
        "the event is still open, running under its third holder"
    );

    fixtures.cleanup().await;
}

/// The log capture reads the current thread alone, so this case drives a
/// current-thread runtime where its siblings take four workers.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_event_ended_at_the_cap_is_logged_as_abandoned_and_never_as_a_refusal() {
    let fixtures = Fixtures::create().await;
    let (seeded, _workspace) = funded(&fixtures).await;
    lapse(&fixtures, &seeded, EVENT_RUNS_MAX).await;
    let assigned = assign(&fixtures, &seeded).await;
    let log = Capture::install();

    let answer = drive(&fixtures, &seeded, assigned).await;

    assert!(answer.is_none(), "a fourth run was issued: {answer:?}");
    let line = log.only("runner_run_abandoned");
    let cap = EVENT_RUNS_MAX.to_string();
    assert_eq!(
        (
            line.field("agentsfleet_turn_id"),
            line.field("label"),
            line.field("earlier_runs"),
            line.field("reason"),
        ),
        (
            Some(seeded.turn_id.as_str()),
            Some(RUN_EXPIRED),
            Some(cap.as_str()),
            Some(DETAIL_RUNS_LAPSED),
        )
    );
    assert!(
        log.events()
            .iter()
            .all(|event| event.field("event") != Some("runner_run_refused")),
        "no gate refused this event, so no refusal is counted for it"
    );
    drop(log);

    fixtures.cleanup().await;
}
