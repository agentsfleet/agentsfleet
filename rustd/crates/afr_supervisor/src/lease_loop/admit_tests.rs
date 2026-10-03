#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::atomic::Ordering;

use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_wire::lease::LeasePayload;
use afr_tools::catalog::{BROWSER, CALCULATOR, FILE_READ, HTTP_REQUEST};

use super::{DETAIL_UNHOSTED, EVENT_UNHOSTED};
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{
    Answer, Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME,
    PROCESSED, RENEWAL_TERMINATE, RUNNER_CRASH, Rig, STARTUP_POSTURE, daemon, lease, position,
    reported,
};

/// The report field a failure's detail rides in.
const FAILURE_DETAIL: &str = "failure_detail";

fn rig() -> Rig {
    behaving(Behaviour::Answer, |_| None)
}

fn behaving(behaviour: Behaviour, special: fn(&Call) -> Option<Answer>) -> Rig {
    Rig::new(
        daemon(special),
        FakeEngine::default(),
        FakeAgent::new(behaviour),
    )
}

/// The test lease, offering exactly `tools`.
fn offering(tools: &[&'static str]) -> LeasePayload<'static> {
    let mut lease = lease(LEASE_ID, FLEET_ID, None);
    lease.policy.tools = tools.iter().map(|&tool| tool.into()).collect();
    lease
}

#[tokio::test(start_paused = true)]
async fn test_unhosted_tool_refuses_lease() {
    let capture = Capture::install();
    let mut rig = rig();

    rig.run(&offering(&[HTTP_REQUEST.name(), BROWSER.name()]))
        .await
        .unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(report[FAILURE_DETAIL], DETAIL_UNHOSTED);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "no model call was made");
    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "no sandbox was built"
    );
    assert_eq!(
        position(&calls, Verb::Hydrate),
        None,
        "refused before anything was prepared for it"
    );
    let refused = capture.only(EVENT_UNHOSTED);
    assert_eq!(refused.level, tracing::Level::ERROR);
    assert_eq!(refused.field("name"), Some(BROWSER.name()));
    assert_eq!(refused.field("lease_id"), Some(LEASE_ID));
    assert_eq!(
        refused.field("error_code"),
        Some(error_code::AGENTSFLEET_INVALID_CONFIG.as_str())
    );
}

#[tokio::test(start_paused = true)]
async fn test_lease_without_sandbox_tools_starts_no_sandbox() {
    let rig = rig();
    rig.run(&offering(&[HTTP_REQUEST.name()])).await.unwrap();
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1, "the turn still ran");

    let mut rig = self::rig();
    rig.run(&offering(&[CALCULATOR.name(), FILE_READ.name()]))
        .await
        .unwrap();
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 1);
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

#[tokio::test(start_paused = true)]
async fn a_supervisor_only_lease_still_reports_and_pushes_memory() {
    let mut rig = rig();

    rig.run(&offering(&[])).await.unwrap();

    let calls = rig.calls();
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert!(position(&calls, Verb::Capture).unwrap() < position(&calls, Verb::Report).unwrap());
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
}

#[tokio::test(start_paused = true)]
async fn should_end_a_supervisor_only_run_when_its_renewal_is_lost() {
    let renewal_lost: fn(&Call) -> Option<Answer> = |call| {
        (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
    };
    let mut rig = behaving(Behaviour::Hang, renewal_lost);

    rig.run(&offering(&[CALCULATOR.name()])).await.unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn should_catch_a_panicking_engine_on_a_supervisor_only_lease() {
    let capture = Capture::install();
    let mut rig = behaving(Behaviour::Panic, |_| None);

    rig.run(&offering(&[CALCULATOR.name()])).await.unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RUNNER_CRASH);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        0,
        "nothing to tear down"
    );
    assert_eq!(capture.only("engine_panicked").level, tracing::Level::ERROR);
}
