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
use opentelemetry::trace::{Tracer as _, TracerProvider as _};
use opentelemetry_sdk::trace::SpanProcessor;

use super::{Builder, LOGS_PATH, METRICS_PATH, TRACES_PATH};
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

/// A freshly built pipeline has lost nothing, on any signal, under either
/// encoding, with and without the log pipeline.
///
/// These counters separate "exporting fine" from "exporting into a void"; a
/// getter wired to the wrong field would be caught by nothing while reporting
/// a lossy process as healthy.
#[tokio::test]
async fn a_new_pipeline_reports_no_losses_on_any_signal() {
    for (encoding, logs) in [(Encoding::HttpProtobuf, true), (Encoding::HttpJson, false)] {
        let config = configured(encoding);
        let registry = Registry::declared().expect("the compiled-in census reads");
        let mut builder = Builder::new(&config, service(), registry);
        if logs {
            builder = builder.with_logs();
        }
        let rendered = format!("{builder:?}");
        assert!(
            rendered.starts_with("Builder") && !rendered.contains(UNREACHABLE),
            "the builder renders without its endpoint's value: {rendered}"
        );

        let (exports, instruments) = builder.install().expect("a well-formed endpoint builds");

        assert_eq!(exports.spans_lost().count(), 0);
        assert_eq!(exports.records_lost().count(), 0);
        assert_eq!(exports.cycles_lost().failed(), 0);
        assert_eq!(
            exports.logger().is_some(),
            logs,
            "the log pipeline exists exactly when it was asked for"
        );
        assert!(
            instruments.unclaimed().len() == instruments.registry().len(),
            "nothing is claimed until a producer claims it"
        );
        // Flushing an empty pipeline against a dead collector delivers
        // nothing and raises nothing: shutdown must stay quiet.
        tokio::task::spawn_blocking(move || exports.flush())
            .await
            .expect("the flush runs to completion");
    }
}

/// A processor that counts the spans it sees end.
#[derive(Debug, Default)]
struct Counting(Arc<AtomicUsize>);

impl SpanProcessor for Counting {
    fn on_start(&self, _span: &mut opentelemetry_sdk::trace::Span, _cx: &opentelemetry::Context) {}

    fn on_end(&self, _span: opentelemetry_sdk::trace::SpanData) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> opentelemetry_sdk::error::OTelSdkResult {
        Ok(())
    }
}

/// A sampler and a span processor the binary adds reach the tracer: with
/// the sampler refusing every span, the processor sees none end.
#[test]
fn a_sampler_and_a_processor_reach_the_tracer() {
    let config = configured(Encoding::HttpJson);
    let registry = Registry::declared().expect("the compiled-in census reads");
    let ended = Arc::new(AtomicUsize::new(0));
    let builder = Builder::new(&config, service(), registry)
        .with_sampler(opentelemetry_sdk::trace::Sampler::AlwaysOff)
        .with_span_processor(Counting(Arc::clone(&ended)));

    let rendered = format!("{builder:?}");
    assert!(rendered.contains("sampled: true"), "{rendered}");
    assert!(rendered.contains("steps: 1"), "{rendered}");
    let (exports, _instruments) = builder.install().expect("the pipeline builds");
    drop(exports.tracer().tracer("a-test").start("refused"));
    assert_eq!(
        ended.load(Ordering::SeqCst),
        0,
        "the sampler was installed, so the processor saw no recorded span end"
    );
    assert_eq!(exports.spans_lost().count(), 0);
}
