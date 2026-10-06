//! The three signal pipelines, built from an accepted configuration.
//!
//! # Why two meter providers
//!
//! The SDK asks the EXPORTER which temporality it wants and aggregates to
//! match. A census declares temporality per family — cost families report
//! windows, runtime families report running totals — so one provider would
//! silently rewrite half of them. Two providers, and the instrument set routes
//! each family by what it declares.
//!
//! # The signal path is appended here, and that is not optional
//!
//! A programmatic endpoint is used verbatim. **Do not "simplify" this by
//! passing a base URL — the crate's own documentation invites exactly that
//! and is wrong.** `opentelemetry-otlp` 0.32's `lib.rs` example sits above
//! `.with_endpoint("http://my-collector:4318")` and claims "the path
//! /v1/traces … is appended automatically". Its code says otherwise:
//! `exporter/http/mod.rs`'s `resolve_http_endpoint` returns a
//! programmatically-provided endpoint verbatim, and reaches
//! `build_endpoint_uri` — the function that appends — only on the
//! `OTEL_EXPORTER_OTLP_ENDPOINT` and default branches. Dropping the append
//! posts every signal to the bare origin, which a collector answers with 404,
//! silently from the process's side. Raised against 0.32.0 — recheck the same
//! function before any bump.

use std::collections::HashMap;
use std::time::Duration;

use afd_observability::metrics::export::{BatchDrops, CountingMetricExporter};
use afd_observability::metrics::instrument::{Instruments, series_ceilings};
use afd_observability::metrics::registry::Registry;
use afd_observability::{CountingExporter, CountingLogExporter, LogDrops, SpanDrops};
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider, ShouldSample, Span, SpanData,
    SpanProcessor,
};

use crate::config::OtlpConfig;
use crate::error::Result;
use crate::resource::Service;

#[cfg(test)]
mod tests;

/// The signal path each exporter posts under.
pub(crate) const TRACES_PATH: &str = "/v1/traces";
pub(crate) const METRICS_PATH: &str = "/v1/metrics";
pub(crate) const LOGS_PATH: &str = "/v1/logs";

/// The signal a flush failure names, by the pipeline it came from.
const SIGNAL_TRACES: &str = "traces";
const SIGNAL_METRICS: &str = "metrics";
const SIGNAL_METRICS_DELTA: &str = "metrics_delta";
const SIGNAL_LOGS: &str = "logs";

/// Spans the batch processor holds before it drops one.
///
/// Pinned rather than inherited: the SDK's default reads `OTEL_BSP_*` from the
/// process environment, so a host variable or an SDK bump would move the
/// number a span budget is sized against with nothing to say so. The value is
/// the SDK's own default, so pinning it changes nothing a deployment sees.
pub const SPAN_QUEUE: usize = 2048;

/// Spans one export carries at most. The SDK's default, pinned.
pub const SPAN_BATCH: usize = 512;

/// How often the batch processor sends what it holds. The SDK's default,
/// pinned.
pub const SPAN_SEND_EVERY: Duration = Duration::from_secs(5);

/// How often the metric reader collects and exports.
///
/// The retired daemon's own maximum flush interval, kept: a series whose
/// points arrive at two different cadences is one whose rate changes at the
/// swap for no reason an operator could act on.
pub const COLLECT_INTERVAL: Duration = Duration::from_secs(5);

/// Everything the transport owns, held so shutdown can flush it.
///
/// Not `Clone`: there is one per process, and a second would be a second set
/// of pipelines exporting the same measurements twice.
#[derive(Debug)]
pub struct Exports {
    tracer: SdkTracerProvider,
    cumulative: SdkMeterProvider,
    delta: SdkMeterProvider,
    /// The log pipeline, where the binary asked for one.
    logger: Option<SdkLoggerProvider>,
    spans_lost: SpanDrops,
    cycles_lost: BatchDrops,
    records_lost: LogDrops,
}

impl Exports {
    /// Delivers everything buffered, for a process that is going away.
    ///
    /// Every signal, and failures are reported rather than raised: this runs
    /// during shutdown, where there is nothing left to abort and a lost batch
    /// is worth a line rather than a non-zero exit. Parks the thread it runs
    /// on; a caller on a reactor moves it to the blocking pool.
    pub fn flush(&self) {
        report_failures([
            (SIGNAL_TRACES, Some(self.tracer.force_flush())),
            (SIGNAL_METRICS, Some(self.cumulative.force_flush())),
            (SIGNAL_METRICS_DELTA, Some(self.delta.force_flush())),
            (
                SIGNAL_LOGS,
                self.logger.as_ref().map(SdkLoggerProvider::force_flush),
            ),
        ]);
    }

    /// Delivers everything buffered and stops every pipeline, for a process
    /// that will not export again.
    ///
    /// A shutdown delivers what it holds, so a caller needs no flush before
    /// it, and the providers' own `Drop` then finds nothing left to do: one
    /// round of exports on the way out, never two. Failures are reported, as
    /// [`Exports::flush`] reports them. Parks the thread it runs on.
    pub fn shutdown(&self) {
        report_failures([
            (SIGNAL_TRACES, Some(self.tracer.shutdown())),
            (SIGNAL_METRICS, Some(self.cumulative.shutdown())),
            (SIGNAL_METRICS_DELTA, Some(self.delta.shutdown())),
            (
                SIGNAL_LOGS,
                self.logger.as_ref().map(SdkLoggerProvider::shutdown),
            ),
        ]);
    }

    /// The logger provider, for the bridge that feeds it log records; absent
    /// where the binary keeps its records on stderr alone.
    #[must_use]
    pub const fn logger(&self) -> Option<&SdkLoggerProvider> {
        self.logger.as_ref()
    }

    /// The tracer provider, for the bridge that feeds it spans.
    #[must_use]
    pub const fn tracer(&self) -> &SdkTracerProvider {
        &self.tracer
    }

    /// Spans this process failed to deliver: the number an operator acts on
    /// when a collector is unreachable, and the reason the export is allowed
    /// to fail quietly.
    #[must_use]
    pub fn spans_lost(&self) -> &SpanDrops {
        &self.spans_lost
    }

    /// Metric collection cycles this process failed to deliver. Cycles rather
    /// than data points: losing one loses a MOMENT, and the next cycle carries
    /// the running total again for every cumulative family.
    #[must_use]
    pub fn cycles_lost(&self) -> &BatchDrops {
        &self.cycles_lost
    }

    /// Log records this process failed to deliver. Zero forever where no log
    /// pipeline was built.
    #[must_use]
    pub fn records_lost(&self) -> &LogDrops {
        &self.records_lost
    }
}

/// Reports each signal whose flush or shutdown failed, once, by name.
///
/// This runs on the way out, where there is nothing left to abort and a lost
/// batch is worth a line rather than a non-zero exit.
fn report_failures(outcomes: [(&'static str, Option<OTelSdkResult>); 4]) {
    for (signal, outcome) in outcomes {
        if let Some(Err(failure)) = outcome {
            let reason = failure.to_string();
            tracing::warn!(
                signal,
                reason,
                event = "telemetry_flush_failed",
                "a signal could not be flushed before shutdown"
            );
        }
    }
}

/// Sees each recorded span the moment it ends, before the batch exporter
/// takes it.
///
/// An observer rather than a second [`SpanProcessor`]: with one processor the
/// SDK MOVES each ended span into it, and with two it deep-clones the span for
/// each — every attribute string a fresh allocation, on every exported span.
/// An observer reads the span by reference inside the one processor, so the
/// span is moved once whatever is watching.
pub trait SpanEnd: Send + Sync + core::fmt::Debug {
    /// `span` ended and was recorded; it goes to the exporter next.
    fn ended(&self, span: &SpanData);
}

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

/// Builds every pipeline for one process, installs the process-wide handles,
/// and claims the instrument set.
///
/// Struct-driven so the two binaries say what differs between them and
/// nothing else: the daemon adds a log pipeline, the runner a span sampler
/// and an observer of its own. What they share — the exporters, the counting
/// wrappers, the two meter providers, the globals — is built once, here.
pub struct Builder<'a> {
    config: &'a OtlpConfig,
    service: Service,
    registry: Registry,
    sampler: Option<Box<dyn ShouldSample>>,
    observers: Vec<Box<dyn SpanEnd>>,
    logs: bool,
    globals: bool,
}

impl core::fmt::Debug for Builder<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Builder")
            .field("config", self.config)
            .field("service", &self.service)
            .field("families", &self.registry.len())
            .field("sampled", &self.sampler.is_some())
            .field("observers", &self.observers.len())
            .field("log_pipeline", &self.logs)
            .field("globals", &self.globals)
            .finish()
    }
}

impl<'a> Builder<'a> {
    /// Pipelines posting under `config`, describing `service`, with the
    /// families `registry` declares.
    #[must_use]
    pub const fn new(config: &'a OtlpConfig, service: Service, registry: Registry) -> Self {
        Self {
            config,
            service,
            registry,
            sampler: None,
            observers: Vec::new(),
            logs: false,
            globals: false,
        }
    }

    /// Decides which spans are recorded; without one every span is.
    #[must_use]
    pub fn with_sampler(mut self, sampler: impl ShouldSample + 'static) -> Self {
        self.sampler = Some(Box::new(sampler));
        self
    }

    /// Sees every recorded span as it ends, ahead of the batch exporter.
    #[must_use]
    pub fn with_span_end(mut self, observer: impl SpanEnd + 'static) -> Self {
        self.observers.push(Box::new(observer));
        self
    }

    /// Builds the log pipeline too, for a binary that bridges its records.
    #[must_use]
    pub const fn with_logs(mut self) -> Self {
        self.logs = true;
        self
    }

    /// Installs the tracer and meter providers as the process-wide ones, for
    /// a binary whose code reaches them through `opentelemetry::global`.
    ///
    /// Opt-in: a process-wide provider is shared state, and a binary that
    /// reads no global has no reason to overwrite one.
    #[must_use]
    pub const fn with_global_providers(mut self) -> Self {
        self.globals = true;
        self
    }

    /// Builds every pipeline, and sets the process-wide tracer and meter when
    /// [`Builder::with_global_providers`] asked for them.
    ///
    /// The globals are set BEFORE the instruments are claimed, so a family
    /// built here is built on the provider this process will actually export
    /// from.
    ///
    /// # Errors
    /// An exporter that will not build from the accepted configuration, or a
    /// census the instrument layer refuses. Both refuse boot: each is a defect
    /// that would otherwise present as a collector receiving nothing.
    pub fn install(self) -> Result<(Exports, Instruments)> {
        let Self {
            config,
            service,
            registry,
            sampler,
            observers,
            logs,
            globals,
        } = self;
        let resource = service.describe();
        let headers: HashMap<String, String> = config.headers().iter().cloned().collect();

        let (tracer, spans_lost) = tracer(config, &resource, &headers, sampler, observers)?;
        let (logger, records_lost) = logger(config, &resource, &headers, logs)?;
        let (cumulative, cycles_lost) = meter_provider(
            config,
            &resource,
            &registry,
            Temporality::Cumulative,
            &headers,
        )?;
        let (delta, _delta_drops) =
            meter_provider(config, &resource, &registry, Temporality::Delta, &headers)?;

        if globals {
            opentelemetry::global::set_tracer_provider(tracer.clone());
            opentelemetry::global::set_meter_provider(cumulative.clone());
        }

        let scope = service.name();
        let instruments = Instruments::new(registry, cumulative.meter(scope), delta.meter(scope));
        let exports = Exports {
            tracer,
            cumulative,
            delta,
            logger,
            spans_lost,
            // The cumulative provider's; the delta provider keeps its own. One
            // number because the question it answers is whether the collector
            // is taking metrics at all, and both readers post to one endpoint.
            cycles_lost,
            records_lost,
        };
        Ok((exports, instruments))
    }
}

/// The span pipeline: the counting exporter behind one batch processor sized
/// by the pinned constants, the binary's observers in front of it, and the
/// sampler the binary added.
fn tracer(
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
fn logger(
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
fn meter_provider(
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
