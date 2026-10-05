//! The pipelines build from an accepted configuration and lose nothing new.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the binaries"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_observability::metrics::registry::Registry;
use afd_observability::semconv;
use opentelemetry::KeyValue;
use opentelemetry::trace::{Link, SpanKind, TraceId, TraceState, Tracer as _, TracerProvider as _};
use opentelemetry_sdk::trace::{SamplingDecision, SamplingResult, ShouldSample, SpanData};

use super::{
    Builder, LOGS_PATH, METRICS_PATH, SPAN_BATCH, SPAN_QUEUE, SPAN_SEND_EVERY, SpanEnd, TRACES_PATH,
};
use crate::config::{Encoding, OTEL_ENDPOINT_KNOB, OtlpConfig};
use crate::resource::Service;

/// A collector nothing is listening on.
///
/// Deliberately unroutable: the transport is BUILT here, never dialled, and
/// an endpoint that resolved would make this test depend on the network.
const UNREACHABLE: &str = "http://127.0.0.1:1";

/// A configuration pointing at the unreachable collector.
fn configured(encoding: Encoding) -> OtlpConfig {
    OtlpConfig::new(UNREACHABLE, OTEL_ENDPOINT_KNOB)
        .expect("an absolute URL")
        .with_encoding(encoding)
        .with_timeout(Duration::from_millis(50))
}

/// The service a test process describes itself as.
fn service() -> Service {
    Service::new(semconv::SCOPE_NAME, "0.0.0-test")
}

/// Every signal carries its `/v1/` path, and the base origin alone is not it.
///
/// The exporter is handed a COMPLETE url, because a programmatically-set
/// endpoint is used verbatim (`resolve_http_endpoint` in
/// `opentelemetry-otlp`). The crate's own example documents the opposite and
/// is wrong, so this asserts the property a reader trusting that example
/// would delete.
#[test]
fn each_signal_posts_under_its_versioned_path() {
    let config = OtlpConfig::new("http://otelcol-dev.internal:4318", OTEL_ENDPOINT_KNOB)
        .expect("an absolute URL");

    for (path, expected) in [
        (TRACES_PATH, "http://otelcol-dev.internal:4318/v1/traces"),
        (METRICS_PATH, "http://otelcol-dev.internal:4318/v1/metrics"),
        (LOGS_PATH, "http://otelcol-dev.internal:4318/v1/logs"),
    ] {
        let built = config.signal_endpoint(path);
        assert_eq!(built, expected);
        assert_ne!(
            built, "http://otelcol-dev.internal:4318",
            "the bare origin is not a signal endpoint"
        );
    }
}

/// Each encoding builds every pipeline, with and without the log pipeline,
/// and the endpoint's value never renders.
#[tokio::test]
async fn either_encoding_builds_every_pipeline() {
    for (encoding, logs) in [(Encoding::HttpProtobuf, true), (Encoding::HttpJson, false)] {
        let config = configured(encoding);
        let registry = Registry::declared().expect("the compiled-in census reads");
        let mut builder = Builder::new(&config, service(), registry);
        if logs {
            builder = builder.with_logs().with_global_providers();
        }
        let rendered = format!("{builder:?}");
        assert!(
            rendered.starts_with("Builder") && !rendered.contains(UNREACHABLE),
            "the builder renders without its endpoint's value: {rendered}"
        );

        let (exports, instruments) = builder.install().expect("a well-formed endpoint builds");

        assert_eq!(
            exports.logger().is_some(),
            logs,
            "the log pipeline exists exactly when it was asked for"
        );
        assert!(
            instruments.unclaimed().len() == instruments.registry().len(),
            "nothing is claimed until a producer claims it"
        );
        // Shutting an empty pipeline down against a dead collector delivers
        // nothing and raises nothing: the way out must stay quiet.
        tokio::task::spawn_blocking(move || exports.shutdown())
            .await
            .expect("the shutdown runs to completion");
    }
}

/// A refused collector counts each loss on its own signal's counter, so a
/// getter wired to the wrong field cannot report a lossy process as healthy.
#[tokio::test]
async fn a_refused_collector_counts_each_loss_on_its_own_signal() {
    use opentelemetry::logs::{LogRecord as _, Logger as _, LoggerProvider as _};

    let config = configured(Encoding::HttpJson);
    let registry = Registry::declared().expect("the compiled-in census reads");
    let (exports, _instruments) = Builder::new(&config, service(), registry)
        .with_logs()
        .install()
        .expect("the pipeline builds");
    let logger = exports.logger().expect("asked for").logger(TEST_SCOPE);
    let mut record = logger.create_log_record();
    record.set_body(LOST.into());
    logger.emit(record);
    drop(exports.tracer().tracer(TEST_SCOPE).start(LOST));

    let exports = tokio::task::spawn_blocking(move || {
        exports.flush();
        exports
    })
    .await
    .expect("the flush runs to completion");

    assert_eq!(
        exports.records_lost().count(),
        1,
        "one record, on the log counter"
    );
    assert_eq!(
        exports.spans_lost().count(),
        1,
        "one span, on the span counter"
    );
}

/// The scope a test's tracer and logger are named for.
const TEST_SCOPE: &str = "a-test";

/// What a test sends to a collector that refuses it.
const LOST: &str = "lost";

/// What [`DropNamed`] refuses.
const REFUSED: &str = "refused";

/// A sampler that drops spans with one name and keeps every other.
#[derive(Debug, Clone)]
struct DropNamed(&'static str);

impl ShouldSample for DropNamed {
    fn should_sample(
        &self,
        _parent_context: Option<&opentelemetry::Context>,
        _trace_id: TraceId,
        name: &str,
        _span_kind: &SpanKind,
        _attributes: &[KeyValue],
        _links: &[Link],
    ) -> SamplingResult {
        SamplingResult {
            decision: if name == self.0 {
                SamplingDecision::Drop
            } else {
                SamplingDecision::RecordAndSample
            },
            attributes: Vec::new(),
            trace_state: TraceState::default(),
        }
    }
}

/// An observer that counts the spans it sees end.
#[derive(Debug, Default)]
struct Counting(Arc<AtomicUsize>);

impl SpanEnd for Counting {
    fn ended(&self, _span: &SpanData) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A sampler and an observer the binary adds both reach the tracer: of two
/// spans, the sampler drops one and the observer sees the other end.
#[test]
fn the_observer_sees_exactly_what_the_sampler_keeps() {
    let config = configured(Encoding::HttpJson);
    let registry = Registry::declared().expect("the compiled-in census reads");
    let ended = Arc::new(AtomicUsize::new(0));
    let (exports, _instruments) = Builder::new(&config, service(), registry)
        .with_sampler(DropNamed(REFUSED))
        .with_span_end(Counting(Arc::clone(&ended)))
        .install()
        .expect("the pipeline builds");

    let tracer = exports.tracer().tracer(TEST_SCOPE);
    drop(tracer.start(REFUSED));
    drop(tracer.start("kept"));

    assert_eq!(
        ended.load(Ordering::SeqCst),
        1,
        "the sampler shed one span and the observer saw the other end"
    );
}

/// The span queue is the pinned one, whatever the environment says: the
/// pinned values are the SDK's own defaults, so a deployment sees no change.
#[test]
fn the_span_queue_is_pinned_at_the_sdks_defaults() {
    assert_eq!(SPAN_QUEUE, 2048);
    assert_eq!(SPAN_BATCH, 512);
    assert_eq!(SPAN_SEND_EVERY, Duration::from_secs(5));
}

/// A series ceiling the SDK refuses refuses the install, as a defect in the
/// build rather than a knob an operator could fix.
#[test]
fn a_ceiling_the_sdk_refuses_refuses_the_install() {
    let config = configured(Encoding::HttpJson);
    let registry = Registry::read(ZERO_CEILING).expect("a zero ceiling reads; the SDK refuses it");

    let refused = Builder::new(&config, service(), registry)
        .install()
        .expect_err("a zero series ceiling cannot build a stream");

    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert_eq!(refused.refused(), None, "no knob is at fault");
    assert!(
        std::error::Error::source(&refused).is_some(),
        "the instrument layer's own sentence survives as the cause"
    );
}

/// A census of one counter whose ceiling is zero.
const ZERO_CEILING: &str = "name\tkind\tnumber\tunit\ttemporality\tlabels\tbounds\tpolicy\tlive_read\tcategory\twatch_for\n\
                            a.family\tcounter\tu64\t1\tcumulative\t-\t-\tfixed:0\tno\ttraffic\tnothing\n";
