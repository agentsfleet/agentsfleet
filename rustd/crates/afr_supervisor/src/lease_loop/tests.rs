#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::id::Uuid7;
use bytes::Bytes;
use tokio::time::Instant;

use super::ACTIVITY_DRAIN_WAIT;
use crate::client::{Call, Verb};
use crate::error;
use crate::renew::RENEWAL_TICK;
use crate::report_spool::ReportSpool;
use crate::test_support::{
    Answer, Behaviour, FAILURE_REASON, FENCING, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME,
    PROCESSED, RENEWAL_TERMINATE, RUNNER_CRASH, Rig, STARTUP_POSTURE, daemon, lease, position,
    reported,
};
use crate::turns::FleetTurns;

/// One answer the test daemon gives before its defaults.
type Special = fn(&Call) -> Option<Answer>;

fn rig(special: Special, engine: FakeEngine, behaviour: Behaviour) -> Rig {
    Rig::new(daemon(special), engine, FakeAgent::new(behaviour))
}

fn healthy(_call: &Call) -> Option<Answer> {
    None
}

fn renewal_lost(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
}

#[tokio::test(start_paused = true)]
async fn test_memory_push_fenced_before_report() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let pushed = position(&calls, Verb::Capture).unwrap();
    assert!(position(&calls, Verb::Hydrate).unwrap() < pushed);
    assert!(
        pushed < position(&calls, Verb::Report).unwrap(),
        "memory is pushed before the report"
    );
    let push: serde_json::Value =
        serde_json::from_slice(calls[pushed].body.as_ref().unwrap()).unwrap();
    assert_eq!(push["fencing_token"], FENCING);
    assert_eq!(push["memory"][0]["key"], "k");
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
    assert!(
        position(&calls, Verb::Activity).is_some(),
        "the run's chunk reached the live tail"
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
    assert!(
        ReportSpool::new(&rig.home)
            .pending()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(start_paused = true)]
async fn renewal_keeps_the_lease_through_a_slow_settle_and_stops_once_answered() {
    let slow_push: Special = |call| {
        (call.verb == Verb::Capture).then(|| {
            Answer::Late(
                RENEWAL_TICK * 3,
                Bytes::from_static(br#"{"stored":1,"skipped":0}"#),
            )
        })
    };
    let mut rig = rig(slow_push, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    tokio::time::sleep(RENEWAL_TICK * 4).await;

    let calls = rig.calls();
    let renewals: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| call.verb == Verb::Renew)
        .map(|(at, _)| at)
        .collect();
    let report = position(&calls, Verb::Report).unwrap();
    assert!(
        renewals.len() >= 2,
        "the push outlasted two ticks: {renewals:?}"
    );
    assert!(
        renewals.iter().all(|at| *at < report),
        "no renewal once the report was answered"
    );
}

#[tokio::test(start_paused = true)]
async fn the_report_settles_before_a_stalled_live_tail_and_the_wait_is_bounded() {
    let stalled: Special = |call| (call.verb == Verb::Activity).then_some(Answer::Stall);
    let mut rig = rig(stalled, FakeEngine::default(), Behaviour::Answer);
    let started = Instant::now();

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let waited = started.elapsed();
    assert!(
        waited >= ACTIVITY_DRAIN_WAIT && waited < ACTIVITY_DRAIN_WAIT * 2,
        "{waited:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_skill_only_bundle_runs_without_one() {
    let absent: Special = |call| {
        (call.verb == Verb::Bundle).then(|| Answer::Fail(error::refused(Verb::Bundle, 404, None)))
    };
    let mut rig = rig(absent, FakeEngine::default(), Behaviour::Answer);
    let digest = "a".repeat(64);

    rig.run(&lease(LEASE_ID, FLEET_ID, Some(&digest)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_sandbox_that_cannot_be_built_refuses_the_lease() {
    let engine = FakeEngine {
        refuse: true,
        ..FakeEngine::default()
    };
    let mut rig = rig(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(
        rig.runs.load(Ordering::SeqCst),
        0,
        "nothing runs without a sandbox"
    );
    assert_eq!(position(&calls, Verb::Capture), None);
}

#[tokio::test(start_paused = true)]
async fn a_4xx_renewal_mid_run_ends_it_and_still_tears_down() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Hang);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(reported(&calls)[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "torn down exactly once"
    );
    assert_eq!(
        position(&calls, Verb::Capture),
        None,
        "no fenced push for an ended lease"
    );
}

/// A cut keeps what the run handed back: the daemon bills its tokens and
/// keeps its memory, and the report names the cut as the reason it ended.
#[tokio::test(start_paused = true)]
async fn a_run_cut_by_its_renewal_still_reports_its_tokens_and_pushes_its_memory() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Stops);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(report["tokens"], 8, "the run's tokens are billed");
    assert_eq!(report["input_tokens"], 3);
    assert_eq!(report["output_tokens"], 4);
    assert!(
        position(&calls, Verb::Capture).is_some(),
        "the run's memory is pushed"
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

/// A run the daemon cut before it answered has only the meter to bill: the
/// renewal that ended it carried its spend, and so does the failed report.
#[tokio::test(start_paused = true)]
async fn a_run_cut_before_it_answered_bills_the_meter_at_renewal_and_in_its_report() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Spends);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let renewal = position(&calls, Verb::Renew).unwrap();
    let renewed: serde_json::Value =
        serde_json::from_slice(calls[renewal].body.as_ref().unwrap()).unwrap();
    assert_eq!(renewed["input_tokens"], 3, "the renewal carried the meter");
    assert_eq!(renewed["cached_input_tokens"], 1);
    assert_eq!(renewed["output_tokens"], 4);
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(report["tokens"], 8, "the failed report bills the meter");
    assert_eq!(report["input_tokens"], 3);
    assert_eq!(report["cached_input_tokens"], 1);
    assert_eq!(report["output_tokens"], 4);
}

#[tokio::test(start_paused = true)]
async fn a_stop_ends_a_lease_in_flight_and_still_tears_down() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Hang);
    let stopping = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        rig.lessee.halt.stop();
    };

    let granted = lease(LEASE_ID, FLEET_ID, None);
    let (ran, ()) = tokio::join!(rig.run(&granted), stopping);

    ran.unwrap();
    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert!(
        report["failure_detail"]
            .as_str()
            .unwrap()
            .contains("told to stop")
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_renewal_refused_before_the_fleet_is_free_never_starts_the_run() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let _busy = turns.claim(&Uuid7::parse(FLEET_ID).unwrap()).await;

    rig.lessee
        .run(&turns, &lease(LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn an_engine_that_fails_or_panics_reports_a_crash_and_is_torn_down() {
    for behaviour in [Behaviour::Break, Behaviour::Panic] {
        let engine = FakeEngine {
            fail_teardown: true,
            ..FakeEngine::default()
        };
        let mut rig = rig(healthy, engine, behaviour);

        rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

        let settled = (
            reported(&rig.calls())[FAILURE_REASON].clone(),
            rig.destroyed.load(Ordering::SeqCst),
        );
        assert_eq!(settled, (RUNNER_CRASH.into(), 1), "{behaviour:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_lease_that_cannot_start_reports_why() {
    let tampered = "a".repeat(64);
    let cases: [(Special, Option<&str>, Verb); 3] = [
        (
            |call| {
                (call.verb == Verb::Bundle).then(|| Answer::Reply(Bytes::from_static(b"not a tar")))
            },
            Some(&tampered),
            Verb::Bundle,
        ),
        (
            |call| {
                (call.verb == Verb::Hydrate)
                    .then(|| Answer::Fail(error::refused(Verb::Hydrate, 403, None)))
            },
            None,
            Verb::Hydrate,
        ),
        (
            |call| (call.verb == Verb::Hydrate).then(|| Answer::Reply(Bytes::from_static(b"["))),
            None,
            Verb::Hydrate,
        ),
    ];
    for (special, bundle, stopped_at) in cases {
        let mut rig = rig(special, FakeEngine::default(), Behaviour::Answer);

        rig.run(&lease(LEASE_ID, FLEET_ID, bundle)).await.unwrap();

        let calls = rig.calls();
        assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
        let setup: Vec<_> = calls
            .iter()
            .map(|call| call.verb)
            .filter(|verb| *verb != Verb::Report)
            .collect();
        assert_eq!(
            setup.last(),
            Some(&stopped_at),
            "it stopped at the step that failed"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_lease_arriving_as_the_pool_shuts_down_is_reported_not_run() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    drop(coordinator);

    rig.lessee
        .run(&turns, &lease(LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_lease_with_an_identifier_out_of_form_is_refused() {
    let rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);

    assert!(rig.run(&lease("nope", FLEET_ID, None)).await.is_err());
}
