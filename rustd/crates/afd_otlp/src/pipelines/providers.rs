//! The three signal providers [`super::Builder::install`] assembles: each
//! one an OTLP/HTTP exporter wrapped in its counting layer, then the SDK
//! provider over it.
//!
//! Split from the module root along the line between configuring a process's
//! telemetry and building one signal's pipeline. The path append the root's
//! note insists on happens here, through `OtlpConfig::signal_endpoint`.

use std::collections::HashMap;
use std::time::Duration;

use afd_observability::metrics::export::{BatchDrops, CountingMetricExporter};
use afd_observability::metrics::instrument::series_ceilings;
use afd_observability::metrics::registry::Registry;
use afd_observability::{CountingExporter, CountingLogExporter, LogDrops, SpanDrops};
use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider, ShouldSample, Span, SpanData,
    SpanProcessor,
};

use super::{
    COLLECT_INTERVAL, LOGS_PATH, METRICS_PATH, SPAN_BATCH, SPAN_QUEUE, SPAN_SEND_EVERY, SpanEnd,
    TRACES_PATH,
};
use crate::config::OtlpConfig;
use crate::error::Result;

/// The batch processor, with the observers a binary added in front of it.
#[derive(Debug)]
struct Observed {
    inner: BatchSpanProcessor,
    observers: Vec<Box<dyn SpanEnd>>,
}

impl SpanProcessor for Observed {
    fn on_start(&self, span: &mut Span, cx: &opentelemetry::Context) {
        self.inner.on_start(span, cx);
    }

    fn on_end(&self, span: SpanData) {
        for observer in &self.observers {
            observer.ended(&span);
        }
        self.inner.on_end(span);
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn shutdown(&self) -> OTelSdkResult {
        self.inner.shutdown()
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

/// The span pipeline: the counting exporter behind one batch processor sized
/// by the pinned constants, the binary's observers in front of it, and the
/// sampler the binary added.
pub(super) fn tracer(
    config: &OtlpConfig,
    resource: &Resource,
    headers: &HashMap<String, String>,
    sampler: Option<Box<dyn ShouldSample>>,
    observers: Vec<Box<dyn SpanEnd>>,
) -> Result<(SdkTracerProvider, SpanDrops)> {
    let spans = CountingExporter::new(
        opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(config.signal_endpoint(TRACES_PATH))
            .with_protocol(config.encoding().wire())
            .with_timeout(config.timeout())
            .with_headers(headers.clone())
            .build()?,
    );
    let spans_lost = spans.drops();
    let batch = BatchSpanProcessor::builder(spans)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_max_queue_size(SPAN_QUEUE)
                .with_max_export_batch_size(SPAN_BATCH)
                .with_scheduled_delay(SPAN_SEND_EVERY)
                .build(),
        )
        .build();
    let mut builder = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_span_processor(Observed {
            inner: batch,
            observers,
        });
    if let Some(sampler) = sampler {
        builder = builder.with_sampler(sampler);
    }
    Ok((builder.build(), spans_lost))
}

/// The log pipeline, where the binary asked for one. Counted like the other
/// two signals; the wrapper does not warn, and `CountingLogExporter` says
/// why.
pub(super) fn logger(
    config: &OtlpConfig,
    resource: &Resource,
    headers: &HashMap<String, String>,
    wanted: bool,
) -> Result<(Option<SdkLoggerProvider>, LogDrops)> {
    if !wanted {
        return Ok((None, LogDrops::new()));
    }
    let logs = CountingLogExporter::new(
        opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_endpoint(config.signal_endpoint(LOGS_PATH))
            .with_protocol(config.encoding().wire())
            .with_timeout(config.timeout())
            .with_headers(headers.clone())
            .build()?,
    );
    let records_lost = logs.drops();
    let logger = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(logs)
        .build();
    Ok((Some(logger), records_lost))
}

/// One meter provider, exporting at `temporality`.
pub(super) fn meter_provider(
    config: &OtlpConfig,
    resource: &Resource,
    registry: &Registry,
    temporality: Temporality,
    headers: &HashMap<String, String>,
) -> Result<(SdkMeterProvider, BatchDrops)> {
    let exporter = CountingMetricExporter::new(
        opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(config.signal_endpoint(METRICS_PATH))
            .with_protocol(config.encoding().wire())
            .with_timeout(config.timeout())
            .with_headers(headers.clone())
            .with_temporality(temporality)
            .build()?,
    );
    let cycles_lost = exporter.drops();
    let provider = SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .with_reader(
            PeriodicReader::builder(exporter)
                .with_interval(COLLECT_INTERVAL)
                .build(),
        )
        .with_view(series_ceilings(registry)?)
        .build();
    Ok((provider, cycles_lost))
}
