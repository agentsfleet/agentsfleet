//! Dimensions 1.1, 1.4 and 1.6: a verb span holds its reached stages once,
//! only declared keys, and stages that account for its length.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use opentelemetry::Value;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SdkTracer, SdkTracerProvider, SpanData, SpanExporter};

use super::{ReportStage, SteerStage, Ticks, VerbClock};
use crate::semconv;

/// A collector that keeps what it is handed.
#[derive(Debug, Default, Clone)]
struct Collected(Arc<Mutex<Vec<SpanData>>>);

impl Collected {
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

/// A tracer exporting straight into `collected`.
fn tracer(collected: &Collected) -> (SdkTracerProvider, SdkTracer) {
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(collected.clone())
        .build();
    let tracer = provider.tracer(semconv::SCOPE_NAME);
    (provider, tracer)
}

/// Clocks the test moves; clones share one reading.
#[derive(Debug, Clone)]
struct Moved {
    base: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl Moved {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: Arc::default(),
        }
    }

    fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap_or_else(PoisonError::into_inner) += by;
    }

    fn offset(&self) -> Duration {
        *self.offset.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Ticks for Moved {
    fn monotonic(&self) -> Instant {
        self.base + self.offset()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000) + self.offset()
    }
}

const EVENT_ID: &str = "01936f00-0000-7000-8000-00000000e7e1";
const FLEET_ID: &str = "01936f00-0000-7000-8000-0000000f1ee7";
const ALL_REPORT_STAGES: [ReportStage; 6] = [
    ReportStage::Load,
    ReportStage::Prepare,
    ReportStage::Capture,
    ReportStage::Commit,
    ReportStage::Announce,
    ReportStage::Ack,
];

fn millis(span: &SpanData, key: &str) -> Option<i64> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == key)
        .and_then(|pair| match pair.value {
            Value::I64(value) => Some(value),
            _ => None,
        })
}

fn stage_keys(span: &SpanData) -> Vec<&str> {
    span.attributes
        .iter()
        .map(|pair| pair.key.as_str())
        .filter(|key| *key != semconv::ATTR_EVENT_ID && *key != semconv::ATTR_AGENT_ID)
        .collect()
}

/// The largest gap a span may leave between its stages and its length: 10%
/// of the span, or 5 ms when that is larger.
fn accounts_for(span_ms: u64, stages_ms: u64) -> bool {
    let allowed = (span_ms / 10).max(5);
    span_ms.saturating_sub(stages_ms) <= allowed
}

/// A report reaching all six stages exports one span with six stages; a report
/// refused at its load exports one span with `load_ms` alone, and a stage
/// marked twice keeps its first reading.
#[test]
fn test_verb_clock_writes_each_reached_stage_once() {
    let collected = Collected::default();
    let (_provider, tracer) = tracer(&collected);
    let ticks = Moved::new();

    let mut whole = VerbClock::<ReportStage, Moved>::start_on(ticks.clone());
    whole.identify(EVENT_ID, FLEET_ID);
    for stage in ALL_REPORT_STAGES {
        ticks.advance(Duration::from_millis(3));
        whole.mark(stage);
    }
    ticks.advance(Duration::from_millis(9));
    whole.mark(ReportStage::Ack);
    whole.record_on(&tracer);

    let mut refused = VerbClock::<ReportStage, Moved>::start_on(ticks.clone());
    ticks.advance(Duration::from_millis(7));
    refused.mark(ReportStage::Load);
    refused.record_on(&tracer);
    refused.record_on(&tracer);

    let spans = collected.spans();
    assert_eq!(spans.len(), 2, "one span per verb, recorded once: {spans:?}");
    let [all, load_only] = [&spans[0], &spans[1]];
    assert_eq!(all.name, semconv::SPAN_FLEET_REPORT);
    assert_eq!(stage_keys(all).len(), ALL_REPORT_STAGES.len());
    assert_eq!(millis(all, semconv::ATTR_REPORT_ACK_MS), Some(3));
    assert_eq!(stage_keys(load_only), [semconv::ATTR_REPORT_LOAD_MS]);
    assert_eq!(millis(load_only, semconv::ATTR_REPORT_LOAD_MS), Some(7));
}

/// A discarded clock, like an empty offer pass, records nothing.
#[test]
fn test_a_discarded_clock_records_no_span() {
    let collected = Collected::default();
    let (_provider, tracer) = tracer(&collected);
    let mut clock = VerbClock::<SteerStage, Moved>::start_on(Moved::new());
    clock.mark(SteerStage::Admit);
    clock.discard();
    clock.record_on(&tracer);
    assert!(collected.spans().is_empty());
}

/// A steer of a probe message exports only declared keys, and no value holds
/// the probe: nothing on the clock can take a message.
#[test]
fn test_verb_span_carries_only_declared_keys() {
    const PROBE: &str = "secret-probe-7f3a";
    let message = format!("deploy {PROBE}");
    let collected = Collected::default();
    let (_provider, tracer) = tracer(&collected);
    let mut clock = VerbClock::<SteerStage, Moved>::start_on(Moved::new());
    assert!(!message.is_empty(), "the steer's message is never handed over");
    clock.identify(EVENT_ID, FLEET_ID);
    clock.mark(SteerStage::Admit);
    clock.mark(SteerStage::Announce);
    clock.record_on(&tracer);

    let spans = collected.spans();
    let span = spans.first().expect("the steer recorded its span");
    for pair in &span.attributes {
        assert!(
            semconv::VERB_SPAN_KEYS.contains(&pair.key.as_str()),
            "`{}` is not a declared verb key",
            pair.key
        );
        assert!(!pair.value.as_str().contains(PROBE));
    }
}

/// Stages summing 96 ms of a 100 ms span account for it; 80 ms of 100 does
/// not, because the gap passes the larger of 10% and 5 ms.
#[test]
fn test_verb_stage_attributes_account_for_the_span() {
    let collected = Collected::default();
    let (_provider, tracer) = tracer(&collected);
    let ticks = Moved::new();
    let mut clock = VerbClock::<SteerStage, Moved>::start_on(ticks.clone());
    ticks.advance(Duration::from_millis(60));
    clock.mark(SteerStage::Admit);
    ticks.advance(Duration::from_millis(36));
    clock.mark(SteerStage::Announce);
    ticks.advance(Duration::from_millis(4));
    clock.record_on(&tracer);

    let spans = collected.spans();
    let span = spans.first().expect("the steer recorded its span");
    let length = span
        .end_time
        .duration_since(span.start_time)
        .expect("the span ends after it starts");
    let span_ms = u64::try_from(length.as_millis()).expect("a short span");
    let stages_ms: i64 = [semconv::ATTR_STEER_ADMIT_MS, semconv::ATTR_STEER_ANNOUNCE_MS]
        .iter()
        .filter_map(|key| millis(span, key))
        .sum();
    let stages_ms = u64::try_from(stages_ms).expect("stages are not negative");
    assert_eq!((span_ms, stages_ms), (100, 96));
    assert!(accounts_for(span_ms, stages_ms));
    assert!(!accounts_for(100, 80), "a 20 ms gap in 100 ms is unaccounted");
}
