//! A restart closes the span so far and times a fresh one, or leaves the clock
//! alone when there is no span to close.

use std::cell::Cell;
use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SpanData;

use super::{Collected, EVENT_ID, FLEET_ID, Moved, millis, tracer};
use crate::semconv;
use crate::verb_span::{OfferStage, VerbClock};

/// The event a span names, if it names one.
fn event_of(span: &SpanData) -> Option<String> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == semconv::ATTR_EVENT_ID)
        .map(|pair| pair.value.to_string())
}

/// A restart records the span so far with its identity, then times a fresh one
/// from zero with none.
#[test]
fn test_a_restart_records_the_first_span_and_starts_a_fresh_one() {
    let collected = Collected::default();
    let (provider, tracer) = tracer(&collected);
    let ticks = Moved::new();
    let mut clock = VerbClock::<OfferStage, Moved>::start_on(ticks.clone());
    clock.identify(EVENT_ID, FLEET_ID);
    ticks.advance(Duration::from_millis(5));
    clock.mark(OfferStage::Claim);
    clock.restart_if_staged_on(|| provider.tracer(semconv::SCOPE_NAME));
    ticks.advance(Duration::from_millis(7));
    clock.mark(OfferStage::Claim);
    clock.record_on(&tracer);

    let spans = collected.spans();
    assert_eq!(spans.len(), 2, "{spans:?}");
    let (first, second) = (
        spans.first().expect("the first"),
        spans.get(1).expect("the second"),
    );
    assert_eq!(millis(first, semconv::ATTR_OFFER_CLAIM_MS), Some(5));
    assert_eq!(event_of(first).as_deref(), Some(EVENT_ID));
    assert_eq!(millis(second, semconv::ATTR_OFFER_CLAIM_MS), Some(7));
    assert_eq!(
        event_of(second),
        None,
        "the fresh span inherits no event: {second:?}"
    );
}

/// A clock that reached no stage has no span to close, so a restart keeps its
/// start and identity: its first stage still counts from where it began.
#[test]
fn test_a_clock_that_reached_no_stage_keeps_its_start_and_identity_on_restart() {
    let collected = Collected::default();
    let (provider, tracer) = tracer(&collected);
    let ticks = Moved::new();
    let mut clock = VerbClock::<OfferStage, Moved>::start_on(ticks.clone());
    clock.identify(EVENT_ID, FLEET_ID);
    ticks.advance(Duration::from_millis(4));
    clock.restart_if_staged_on(|| provider.tracer(semconv::SCOPE_NAME));
    ticks.advance(Duration::from_millis(6));
    clock.mark(OfferStage::Claim);
    clock.record_on(&tracer);

    let spans = collected.spans();
    assert_eq!(spans.len(), 1, "{spans:?}");
    let span = spans.first().expect("the one span");
    assert_eq!(millis(span, semconv::ATTR_OFFER_CLAIM_MS), Some(10));
    assert_eq!(event_of(span).as_deref(), Some(EVENT_ID));
}

/// An untimed clock restarts without recording and without asking for a
/// tracer: nothing it marks reaches a collector, here or through the global
/// provider.
#[test]
fn test_an_untimed_clock_restarts_without_recording_or_asking_for_a_tracer() {
    let collected = Collected::default();
    let (provider, tracer) = tracer(&collected);
    let asked = Cell::new(false);
    let mut untimed = VerbClock::<OfferStage>::untimed();
    untimed.mark(OfferStage::Claim);
    untimed.restart_if_staged_on(|| {
        asked.set(true);
        provider.tracer(semconv::SCOPE_NAME)
    });
    untimed.mark(OfferStage::Claim);
    untimed.record_on(&tracer);

    assert!(!asked.get(), "an untimed clock asked for a tracer");
    assert_eq!(
        collected.spans().len(),
        0,
        "an untimed clock recorded a span"
    );
}
