#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::test_util::trace::Capture;

use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME, PROCESSED, Rig, daemon,
    lease, position, reported,
};

/// The report field the trace rides in, and the list inside it and a post.
const TRACE_FIELD: &str = "tool_calls";
const CALLS: &str = "calls";

fn rig(special: fn(&Call) -> Option<Answer>) -> Rig {
    Rig::new(
        daemon(special),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Calls),
    )
}

#[tokio::test(start_paused = true)]
async fn test_records_post_before_report() {
    let mut rig = rig(|_| None);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let records = position(&calls, Verb::Records).unwrap();
    assert!(
        records < position(&calls, Verb::Capture).unwrap(),
        "records go before memory"
    );
    assert!(
        records < position(&calls, Verb::Report).unwrap(),
        "and before the report"
    );
    assert!(
        calls[records]
            .path
            .ends_with(&format!("/{LEASE_ID}/tool-calls"))
    );
    let posted: serde_json::Value =
        serde_json::from_slice(calls[records].body.as_ref().unwrap()).unwrap();
    assert_eq!(posted["fencing_token"], 504);
    assert_eq!(posted[CALLS].as_array().unwrap().len(), 3);
    let report = reported(&calls);
    assert_eq!(report[OUTCOME], PROCESSED);
    assert_eq!(report[TRACE_FIELD][CALLS].as_array().unwrap().len(), 3);
    assert_eq!(report[TRACE_FIELD][CALLS][0]["call_id"], "1");
}

#[tokio::test(start_paused = true)]
async fn a_failed_records_post_leaves_the_report_untouched() {
    let capture = Capture::install();
    let mut rig = rig(|call| {
        (call.verb == Verb::Records).then(|| Answer::Fail(error::refused(Verb::Records, 409, None)))
    });

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[OUTCOME], PROCESSED);
    assert_eq!(
        report[TRACE_FIELD][CALLS].as_array().unwrap().len(),
        3,
        "the trace still rides"
    );
    assert!(
        position(&calls, Verb::Capture).is_some(),
        "memory is still pushed"
    );
    let failed = capture.only("tool_records_post_failed");
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(failed.field("lease_id"), Some(LEASE_ID));
}

#[tokio::test(start_paused = true)]
async fn a_run_with_no_calls_posts_no_records_and_no_trace() {
    let mut rig = Rig::new(
        daemon(|_| None),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(position(&calls, Verb::Records), None);
    assert!(reported(&calls).get(TRACE_FIELD).is_none());
}
