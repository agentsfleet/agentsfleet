#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::test_util::trace::{Capture, CapturedSpan};
use afd_observability::semconv::{
    ATTR_AGENT_ID, ATTR_LEASE_ID, ATTR_RUNNER_HOST, ATTR_RUNNER_ID, RUNNER_SCOPE_NAME,
    SPAN_RUNNER_LEASE,
};
use afd_wire::lease::LeasePayload;

use super::EVENT_IDENTITY_FAILED;
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME, PROCESSED, RUNNER_HOST,
    RUNNER_ID, Rig, daemon, lease, reported,
};

/// The second lease every repeat suite runs.
const SECOND_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a805a";

fn rig(special: fn(&Call) -> Option<Answer>) -> Rig {
    Rig::new(
        daemon(special),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    )
}

/// The lease spans a capture saw.
fn lease_spans(capture: &Capture) -> Vec<CapturedSpan> {
    let spans = capture.spans().into_iter();
    spans
        .filter(|span| span.name == SPAN_RUNNER_LEASE)
        .collect()
}

fn offered() -> LeasePayload<'static> {
    let mut leased = lease(LEASE_ID, FLEET_ID, None);
    leased.policy.tools = Vec::new();
    leased
}

#[tokio::test(start_paused = true)]
async fn every_lease_runs_inside_a_span_naming_this_runner() {
    let capture = Capture::install();
    let mut rig = rig(|_| None);

    rig.run(&offered()).await.unwrap();

    let spans = lease_spans(&capture);
    assert_eq!(spans.len(), 1, "{spans:?}");
    let span = &spans[0];
    assert_eq!(
        span.target, RUNNER_SCOPE_NAME,
        "a runner span, never the daemon's"
    );
    assert_eq!(span.field(ATTR_RUNNER_ID), Some(RUNNER_ID));
    assert_eq!(span.field(ATTR_RUNNER_HOST), Some(RUNNER_HOST));
    assert_eq!(span.field(ATTR_LEASE_ID), Some(LEASE_ID));
    assert_eq!(span.field(ATTR_AGENT_ID), Some(FLEET_ID));
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

#[tokio::test(start_paused = true)]
async fn the_daemon_is_asked_once_whatever_the_leases() {
    let mut rig = rig(|_| None);
    let mut second = offered();
    second.lease_id = SECOND_LEASE.into();

    rig.run(&offered()).await.unwrap();
    rig.run(&second).await.unwrap();

    let asked = rig
        .calls()
        .iter()
        .filter(|call| call.verb == Verb::Me)
        .count();
    assert_eq!(asked, 1);
}

#[tokio::test(start_paused = true)]
async fn a_daemon_that_cannot_say_costs_the_span_its_runner_and_nothing_else() {
    let capture = Capture::install();
    let mut rig = rig(|call| {
        (call.verb == Verb::Me).then(|| Answer::Fail(error::unavailable(Verb::Me, 503)))
    });

    rig.run(&offered()).await.unwrap();

    let spans = lease_spans(&capture);
    assert_eq!(spans[0].field(ATTR_RUNNER_ID), None);
    assert_eq!(spans[0].field(ATTR_LEASE_ID), Some(LEASE_ID));
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED, "the lease ran");
    let failed = capture.only(EVENT_IDENTITY_FAILED);
    assert_eq!(failed.level, tracing::Level::WARN);
    assert!(failed.field("error_code").is_some());
}
