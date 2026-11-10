//! The lease's span carries the segments it reached, and only
//! those, under the keys `semconv` declares.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use af_core::test_util::trace::{Capture, CapturedSpan};
use af_observability::semconv::{
    ATTR_RUNNER_MEMORY_MS, ATTR_RUNNER_MODEL_MS, ATTR_RUNNER_REPORT_MS, ATTR_RUNNER_SANDBOX_MS,
    ATTR_RUNNER_TOOLS_MS, RUNNER_SEGMENT_KEYS, SPAN_RUNNER_LEASE,
};

use super::super::EVENT_COMPLETED;
use super::Segments;
use crate::identity::lease_span;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, Rig, daemon, lease,
};

/// The keys the lease's last line logs the same segments under.
const LINE_MODEL_MS: &str = "model_ms";
const LINE_REPORT_MS: &str = "report_ms";

/// The one lease span the capture saw.
fn the_lease_span(capture: &Capture) -> CapturedSpan {
    let spans: Vec<CapturedSpan> = capture
        .spans()
        .into_iter()
        .filter(|span| span.name == SPAN_RUNNER_LEASE)
        .collect();
    assert_eq!(spans.len(), 1, "one lease opens one span: {spans:?}");
    spans.into_iter().next().unwrap()
}

/// A lease that hydrated, ran its model and reported, and reached nothing
/// else, writes those three segments and leaves every other key off the span.
#[test]
fn test_lease_span_records_existing_segments() {
    let capture = Capture::install();
    let span = lease_span(None, &lease(LEASE_ID, FLEET_ID));
    let segments = Segments {
        memory: Some(Duration::from_millis(12)),
        model: Some(Duration::from_millis(400)),
        report: Some(Duration::from_millis(30)),
        ..Segments::default()
    };

    segments.record(&span);

    let recorded = the_lease_span(&capture);
    assert_eq!(recorded.field(ATTR_RUNNER_MEMORY_MS), Some("12"));
    assert_eq!(recorded.field(ATTR_RUNNER_MODEL_MS), Some("400"));
    assert_eq!(recorded.field(ATTR_RUNNER_REPORT_MS), Some("30"));
    assert_eq!(recorded.field(ATTR_RUNNER_SANDBOX_MS), None);
    assert_eq!(recorded.field(ATTR_RUNNER_TOOLS_MS), None);
    let written = RUNNER_SEGMENT_KEYS
        .iter()
        .filter(|key| recorded.field(key).is_some())
        .count();
    assert_eq!(
        written, 3,
        "only reached segments are written: {recorded:?}"
    );
}

/// Each of the ten segments lands under its own key: distinct durations make
/// a key paired with the wrong segment fail.
#[test]
fn test_each_segment_lands_under_its_own_key() {
    let capture = Capture::install();
    let span = lease_span(None, &lease(LEASE_ID, FLEET_ID));
    let ms = Duration::from_millis;
    let segments = Segments {
        memory: Some(ms(1)),
        sandbox: Some(ms(2)),
        egress: Some(ms(3)),
        prepare: Some(ms(4)),
        checkout: Some(ms(5)),
        sandbox_wait: Some(ms(6)),
        model: Some(ms(7)),
        tools: Some(ms(8)),
        report: Some(ms(9)),
        first_token: Some(ms(10)),
    };

    segments.record(&span);

    let recorded = the_lease_span(&capture);
    let read: Vec<Option<&str>> = RUNNER_SEGMENT_KEYS
        .iter()
        .map(|key| recorded.field(key))
        .collect();
    let expected: Vec<String> = (1..=10).map(|n: u32| n.to_string()).collect();
    let expected: Vec<Option<&str>> = expected.iter().map(|n| Some(n.as_str())).collect();
    assert_eq!(read, expected, "segments in RUNNER_SEGMENT_KEYS order");
}

/// A lease run to its report writes onto its span the segments its last line
/// logs, with the same values: the span and the journal never disagree.
#[tokio::test(start_paused = true)]
async fn test_a_concluded_lease_writes_its_segments_onto_its_span() {
    let capture = Capture::install();
    let rig = Rig::new(
        daemon(|_call| None),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let line = capture.only(EVENT_COMPLETED);
    let span = the_lease_span(&capture);
    assert!(span.field(ATTR_RUNNER_MODEL_MS).is_some(), "{span:?}");
    assert_eq!(span.field(ATTR_RUNNER_MODEL_MS), line.field(LINE_MODEL_MS));
    assert_eq!(
        span.field(ATTR_RUNNER_REPORT_MS),
        line.field(LINE_REPORT_MS)
    );
}
