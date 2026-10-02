#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::id::Uuid7;
use afd_wire::memory::MemoryHydrateResponse;
use afr_sandbox::Limits;
use bytes::Bytes;

use super::Lessee;
use crate::bundles::BundleCache;
use crate::client::{Call, Verb};
use crate::error;
use crate::report_spool::{Delivery, ReportSpool};
use crate::storage_home::StorageHome;
use crate::test_support::{
    Answer, Behaviour, FENCING, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, drain, json, lease,
    plane,
};
use crate::turns::FleetTurns;

/// The report field a failed run names its class in.
const FAILURE_REASON: &str = "failure_reason";
/// The class a sandbox that would not build reports.
const STARTUP_POSTURE: &str = "startup_posture";
/// The class a lease the daemon ended mid-run reports.
const RENEWAL_TERMINATE: &str = "renewal_terminate";

/// The daemon every lease test talks to; `special` answers first when it has an answer.
fn daemon(
    special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    move |call| {
        special(call).unwrap_or_else(|| match call.verb {
            Verb::Hydrate => json(&MemoryHydrateResponse { memory: Vec::new() }),
            Verb::Capture => json(&serde_json::json!({"stored": 1, "skipped": 0})),
            _ => json(&serde_json::json!({"ok": true, "lease_expires_at": 1})),
        })
    }
}

/// One answer the test daemon gives before its defaults.
type Special = fn(&Call) -> Option<Answer>;

struct Rig {
    _root: tempfile::TempDir,
    lessee: Lessee,
    home: StorageHome,
    calls: tokio::sync::mpsc::UnboundedReceiver<Call>,
    runs: Arc<AtomicUsize>,
    prepared: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
}

fn rig(
    answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
    engine: FakeEngine,
    behaviour: Behaviour,
) -> Rig {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let (plane, calls) = plane(answer);
    let agent = FakeAgent::new(behaviour);
    let (runs, prepared, destroyed) = (
        Arc::clone(&agent.runs),
        Arc::clone(&engine.prepared),
        Arc::clone(&engine.destroyed),
    );
    let lessee = Lessee {
        plane,
        engine: Box::new(engine),
        agent: Box::new(agent),
        spool: ReportSpool::new(&home),
        bundles: BundleCache::new(&home),
        limits: Limits::default(),
    };
    Rig {
        _root: root,
        lessee,
        home,
        calls,
        runs,
        prepared,
        destroyed,
    }
}

impl Rig {
    async fn run(&mut self) -> (crate::Result<Option<Delivery>>, Vec<Call>) {
        let (turns, coordinator) = FleetTurns::start();
        tokio::spawn(coordinator);
        let outcome = self
            .lessee
            .run(&turns, &lease(LEASE_ID, FLEET_ID, None))
            .await;
        (outcome, drain(&mut self.calls))
    }
}

fn reported(calls: &[Call]) -> serde_json::Value {
    let report = calls
        .iter()
        .rfind(|call| call.verb == Verb::Report)
        .unwrap();
    serde_json::from_slice(report.body.as_ref().unwrap()).unwrap()
}

fn position(calls: &[Call], verb: Verb) -> Option<usize> {
    calls.iter().position(|call| call.verb == verb)
}

#[tokio::test(start_paused = true)]
async fn test_memory_push_fenced_before_report() {
    let mut rig = rig(daemon(|_| None), FakeEngine::default(), Behaviour::Answer);

    let (outcome, calls) = rig.run().await;

    assert_eq!(outcome.unwrap(), Some(Delivery::Accepted));
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
    assert_eq!(reported(&calls)["outcome"], "processed");
    assert!(
        position(&calls, Verb::Activity).is_some(),
        "the run's chunk reached the live tail"
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
    assert!(ReportSpool::new(&rig.home).pending().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_sandbox_that_cannot_be_built_refuses_the_lease() {
    let engine = FakeEngine {
        refuse: true,
        ..FakeEngine::default()
    };
    let mut rig = rig(daemon(|_| None), engine, Behaviour::Answer);

    let (outcome, calls) = rig.run().await;

    assert!(outcome.is_ok());
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
    let lost = |call: &Call| {
        (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
    };
    let mut rig = rig(daemon(lost), FakeEngine::default(), Behaviour::Hang);

    let (_outcome, calls) = rig.run().await;

    assert_eq!(reported(&calls)[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1);
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

#[tokio::test(start_paused = true)]
async fn a_renewal_refused_before_the_fleet_is_free_never_starts_the_run() {
    let lost = |call: &Call| {
        (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
    };
    let mut rig = rig(daemon(lost), FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let _busy = turns.claim(&Uuid7::parse(FLEET_ID).unwrap()).await;

    rig.lessee
        .run(&turns, &lease(LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();

    assert_eq!(
        reported(&drain(&mut rig.calls))[FAILURE_REASON],
        RENEWAL_TERMINATE
    );
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn an_engine_failure_reports_a_crash_and_a_failed_teardown_is_only_logged() {
    let engine = FakeEngine {
        fail_teardown: true,
        ..FakeEngine::default()
    };
    let mut rig = rig(daemon(|_| None), engine, Behaviour::Break);

    let (outcome, calls) = rig.run().await;

    assert!(outcome.is_ok());
    assert_eq!(reported(&calls)[FAILURE_REASON], "runner_crash");
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_lease_that_cannot_start_reports_why() {
    let digest = "a".repeat(64);
    let cases: [(Special, Option<&str>, Verb); 3] = [
        (
            |call| {
                (call.verb == Verb::Bundle)
                    .then(|| Answer::Fail(error::refused(Verb::Bundle, 404, None)))
            },
            Some(&digest),
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
    for (case, bundle, last_setup_call) in cases {
        let mut rig = rig(daemon(case), FakeEngine::default(), Behaviour::Answer);
        let (turns, coordinator) = FleetTurns::start();
        tokio::spawn(coordinator);

        rig.lessee
            .run(&turns, &lease(LEASE_ID, FLEET_ID, bundle))
            .await
            .unwrap();

        let calls = drain(&mut rig.calls);
        assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
        let setup: Vec<_> = calls
            .iter()
            .map(|call| call.verb)
            .filter(|verb| *verb != Verb::Report)
            .collect();
        assert_eq!(
            setup.last(),
            Some(&last_setup_call),
            "it stopped at the step that failed"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_failed_memory_push_still_reports_and_an_unposted_report_stays_spooled() {
    let flaky = |call: &Call| match call.verb {
        Verb::Capture => Some(Answer::Fail(error::refused(Verb::Capture, 409, None))),
        Verb::Report => Some(Answer::Fail(error::unavailable(Verb::Report, 503))),
        _ => None,
    };
    let mut rig = rig(daemon(flaky), FakeEngine::default(), Behaviour::Answer);

    let (outcome, calls) = rig.run().await;

    assert_eq!(outcome.unwrap(), None, "kept, not lost");
    assert!(position(&calls, Verb::Report).is_some());
    assert_eq!(ReportSpool::new(&rig.home).pending().unwrap().len(), 1);
}

#[tokio::test]
async fn a_lease_with_an_identifier_out_of_form_is_refused() {
    let rig = rig(daemon(|_| None), FakeEngine::default(), Behaviour::Answer);
    let (turns, _coordinator) = FleetTurns::start();

    assert!(
        rig.lessee
            .run(&turns, &lease("nope", FLEET_ID, None))
            .await
            .is_err()
    );
}
