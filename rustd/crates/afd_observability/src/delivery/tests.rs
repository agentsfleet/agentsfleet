//! Dimension 1.2 — the delivery span says what happened, and when it happened.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use opentelemetry::trace::{SpanId, TracerProvider as _};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};

use super::{Delivery, MAX_RUN};
use crate::semconv;

/// A collector that keeps what it is handed.
#[derive(Debug, Default, Clone)]
struct Collected(Arc<Mutex<Vec<SpanData>>>);

impl Collected {
    /// Every span exported so far.
    fn spans(&self) -> Vec<SpanData> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl SpanExporter for Collected {
    fn export(&self, batch: Vec<SpanData>) -> impl Future<Output = OTelSdkResult> + Send {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(batch);
        std::future::ready(Ok(()))
    }
}

/// How far past the epoch a report settles, in these tests.
///
/// Far enough that a run of any length still starts after the epoch, so a test
/// about durations is never accidentally a test about the epoch — which is
/// what [`just_after_the_epoch`] is for instead.
const SETTLED_AT_SECONDS: u64 = 1_700_000_000;

/// One second, as the whole distance between the epoch and a settle no run
/// can fit before.
const A_SECOND: Duration = Duration::from_secs(1);

/// When a report settles, in these tests.
///
/// A function rather than a `const`: `SystemTime` arithmetic is not available
/// in a constant, because the epoch's relationship to the platform clock is
/// not something the compiler can evaluate.
fn settled_at() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(SETTLED_AT_SECONDS)
}

/// A settle instant no run can fit before.
///
/// Subtract any believable duration from it and the start is a moment OTLP
/// cannot carry.
fn just_after_the_epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH + A_SECOND
}

/// One settled run, with every field distinguishable from every other.
fn delivery() -> Delivery<'static> {
    Delivery {
        tenant_id: "01936f00-0000-7000-8000-000000007e11",
        workspace_id: "01936f00-0000-7000-8000-000000005402",
        fleet_id: "01936f00-0000-7000-8000-0000000f1ee7",
        event_id: "01936f00-0000-7000-8000-00000000e7e1",
        posture: "self_managed",
        provider: "Anthropic",
        model: "claude-fable-5-1",
        input_tokens: 4_096,
        output_tokens: 512,
        wall: Duration::from_secs(90),
    }
}

/// Records `delivery` through a real provider and answers what was exported.
fn recorded(delivery: &Delivery<'_>, completed_at: SystemTime) -> Vec<SpanData> {
    let collector = Collected::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(collector.clone())
        .build();
    delivery.record_on(&provider.tracer(semconv::SCOPE_NAME), completed_at);
    let _flushed = provider.force_flush();
    collector.spans()
}

/// The one span a delivery produces.
fn only_span(spans: Vec<SpanData>) -> SpanData {
    let mut spans = spans;
    assert_eq!(spans.len(), 1, "one delivery is one span");
    spans.pop().expect("the length was just asserted")
}

/// The value of `key` on `span`, rendered.
fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| attribute.value.to_string())
}

/// The span carries exactly the declared attributes, and each says its fact.
///
/// The set is compared BOTH ways against `DELIVERY_SPAN_KEYS`. An attribute
/// emitted and not declared is a key no test knows to check; a key declared
/// and not emitted is a dashboard column that is empty forever.
#[test]
fn the_delivery_span_carries_every_declared_key() {
    let delivery = delivery();
    let span = only_span(recorded(&delivery, settled_at()));

    assert_eq!(span.name, semconv::SPAN_FLEET_DELIVERY);
    for key in semconv::DELIVERY_SPAN_KEYS {
        assert!(
            attribute(&span, key).is_some(),
            "the span declares `{key}` and did not carry it"
        );
    }
    for carried in &span.attributes {
        assert!(
            semconv::DELIVERY_SPAN_KEYS.contains(&carried.key.as_str()),
            "the span carried `{}`, which nothing declares",
            carried.key
        );
    }

    assert_eq!(
        attribute(&span, semconv::ATTR_OPERATION_NAME).as_deref(),
        Some(semconv::OPERATION_INVOKE_AGENT)
    );
    assert_eq!(
        attribute(&span, semconv::ATTR_AGENT_ID).as_deref(),
        Some(delivery.fleet_id),
        "the fleet IS the agent identity in this product"
    );
    assert_eq!(
        attribute(&span, semconv::ATTR_PROVIDER_NAME).as_deref(),
        Some("anthropic"),
        "the configured spelling is normalised to the well-known one"
    );
    assert_eq!(
        attribute(&span, semconv::ATTR_USAGE_INPUT_TOKENS).as_deref(),
        Some("4096")
    );
    assert_eq!(
        attribute(&span, semconv::ATTR_TENANT_ID).as_deref(),
        Some(delivery.tenant_id)
    );
}

/// The span is retro-dated: it ends when the report settled and starts a run
/// earlier.
///
/// The property the whole module exists for. A span timed around the HTTP call
/// that delivered the report would pass every attribute assertion above and
/// report a ninety-second run as four milliseconds.
#[test]
fn the_delivery_span_is_retro_dated_from_the_reported_duration() {
    let delivery = delivery();
    let completed_at = settled_at();
    let span = only_span(recorded(&delivery, completed_at));

    assert_eq!(span.end_time, completed_at, "the run ended when it settled");
    assert_eq!(
        span.start_time,
        completed_at - delivery.wall,
        "the start is the settle less the reported duration"
    );
    assert_eq!(
        span.end_time
            .duration_since(span.start_time)
            .expect("the span ends after it starts"),
        delivery.wall
    );
}

/// A run reported as impossibly long is capped rather than believed.
///
/// The duration is runner-supplied. Uncapped, one wrong reading puts a span's
/// start years in the past and drags a trace's whole timeline with it — and
/// nobody reading the backend afterwards can filter out what they cannot see.
#[test]
fn an_implausible_duration_is_capped_and_the_span_still_lands() {
    let delivery = Delivery {
        wall: MAX_RUN * 4,
        ..delivery()
    };
    let completed_at = settled_at();
    let span = only_span(recorded(&delivery, completed_at));

    assert_eq!(
        span.start_time,
        completed_at - MAX_RUN,
        "the start is pulled back to the cap, not to the reported duration"
    );
    assert_eq!(
        span.end_time, completed_at,
        "the delivery is still recorded — the cap loses precision, never the span"
    );
}

/// A duration that would start the run before the epoch records nothing.
///
/// Not a panic and not a span at time zero: a run this daemon cannot place in
/// time is one an operator would search for and never find, so it is refused
/// with a reason instead of exported as a lie.
#[test]
fn a_start_before_the_epoch_records_no_span() {
    let delivery = Delivery {
        wall: Duration::from_secs(60),
        ..delivery()
    };
    let spans = recorded(&delivery, just_after_the_epoch());

    assert!(
        spans.is_empty(),
        "an unplaceable start is skipped, not recorded at an invented time"
    );
}

/// The span is a root, never a child of the request that delivered it.
///
/// The report arrives AFTER the run finished, so a parent-child link would
/// describe a child that began before its parent — which a trace backend reads
/// as a clock fault and drops or flags.
#[test]
fn the_delivery_span_is_a_root() {
    let span = only_span(recorded(&delivery(), settled_at()));

    assert_eq!(
        span.parent_span_id,
        SpanId::INVALID,
        "a delivery span has no parent — see the module note"
    );
}

/// A vendor with no well-known spelling is omitted, and the rest still lands.
#[test]
fn an_unmapped_provider_is_omitted_rather_than_exported() {
    let delivery = Delivery {
        provider: "our-internal-gateway",
        ..delivery()
    };
    let span = only_span(recorded(&delivery, settled_at()));

    assert_eq!(
        attribute(&span, semconv::ATTR_PROVIDER_NAME),
        None,
        "a private spelling under a standard key would tell every consumer the \
         standard vocabulary contains a word it does not"
    );
    assert_eq!(
        attribute(&span, semconv::ATTR_REQUEST_MODEL).as_deref(),
        Some(delivery.model),
        "the omission costs one attribute, not the span"
    );
}

/// Every attribute carries the field it is named for.
///
/// The key-set test above proves each declared key is present; it cannot tell
/// two `KeyValue::new` lines whose keys were swapped from two that were not.
/// A workspace id filed under the event key is a span that passes every set
/// comparison and sends an operator to the wrong tenant.
#[test]
fn every_attribute_carries_the_field_it_is_named_for() {
    let delivery = delivery();
    let span = only_span(recorded(&delivery, settled_at()));

    for (key, expected) in [
        (semconv::ATTR_USAGE_OUTPUT_TOKENS, "512"),
        (semconv::ATTR_EXECUTION_POSTURE, delivery.posture),
        (semconv::ATTR_WORKSPACE_ID, delivery.workspace_id),
        (semconv::ATTR_EVENT_ID, delivery.event_id),
        (semconv::ATTR_REQUEST_MODEL, delivery.model),
    ] {
        assert_eq!(
            attribute(&span, key).as_deref(),
            Some(expected),
            "`{key}` carried something other than the field it names"
        );
    }
}

/// A count past the signed range saturates rather than wrapping.
///
/// The counts are runner-supplied, so the daemon does not get to assume they
/// are sane. Wrapping would publish a negative token usage, which is a number
/// every dashboard will happily average.
#[test]
fn a_count_past_the_signed_range_saturates_rather_than_wrapping() {
    let delivery = Delivery {
        input_tokens: u64::MAX,
        ..delivery()
    };
    let span = only_span(recorded(&delivery, settled_at()));

    assert_eq!(
        attribute(&span, semconv::ATTR_USAGE_INPUT_TOKENS).as_deref(),
        Some(i64::MAX.to_string().as_str()),
        "an absurd count is capped at the largest attribute value, never wrapped"
    );
}
