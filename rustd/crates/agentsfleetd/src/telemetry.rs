//! The transport: what turns recorded telemetry into telemetry that has left.
//!
//! Everything under `afd_observability` records into machinery that goes
//! nowhere on its own. This is the endpoint half — the piece that crate
//! deliberately does not carry, because an endpoint is configuration and a
//! library that held one would be a library with a deployment inside it.
//!
//! # Three signals, three exporters, one bound
//!
//! Each signal gets its own exporter and each is wrapped in the counting
//! wrapper `afd_observability` shipped for it. That wrapper is the whole
//! failure posture: a collector that is down costs dropped batches and a
//! counter that says so, never latency on a request. Nothing here is allowed
//! to change that, which is why the transport plugs INTO the wrapper rather
//! than beside it.
//!
//! # Why two meter providers
//!
//! The SDK asks the EXPORTER which temporality it wants and aggregates to
//! match. The census declares temporality per family — the cost families
//! report windows, the runtime families report running totals — so one
//! provider would silently rewrite half of them. Two providers, and the
//! registry routes each family by what it declares.
//!
//! # What boot does NOT do here
//!
//! Read the environment. Knobs are `preflight`'s and arrive resolved, so this
//! module cannot disagree with the refusal that already happened.

use std::collections::HashMap;
use std::time::Duration;

use afd_observability::metrics::instrument::{Instruments, series_ceilings};
use afd_observability::metrics::registry::Registry;
use afd_observability::producers::{self, GaugeSources};
use afd_observability::metrics::export::BatchDrops;
use afd_observability::{CountingExporter, SpanDrops, semconv};
use afd_observability::metrics::export::CountingMetricExporter;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_otlp::{Protocol, WithExportConfig as _, WithHttpConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};
use opentelemetry_sdk::trace::SdkTracerProvider;

use crate::error::BootFailure;
use crate::preflight::OtlpConfig;

mod resource;

#[cfg(test)]
mod tests;

pub use self::resource::{INSTANCE_ID_KNOB, MACHINE_ID_KNOB};

pub(crate) use self::resource::resident_bytes;

/// The signal path each exporter posts under.
///
/// Appended here because a programmatic endpoint is used verbatim: the
/// exporter only derives these when it reads the environment itself, and this
/// daemon reads the environment in exactly one place, which is `preflight`.
const TRACES_PATH: &str = "/v1/traces";
const METRICS_PATH: &str = "/v1/metrics";
const LOGS_PATH: &str = "/v1/logs";

/// How often the metric reader collects and exports.
///
/// The Zig exporter's own maximum flush interval, kept: the two daemons write
/// into the same store during the cutover, and a series whose points arrive at
/// two different cadences is one whose rate changes at the swap for no reason
/// an operator could act on.
const COLLECT_INTERVAL: Duration = Duration::from_secs(5);

/// The wire encoding this build sends, as configuration spells it.
const PROTOCOL_JSON: &str = "http/json";

/// Everything the transport owns, held so shutdown can flush it.
///
/// Not `Clone`: there is one per process, and a second would be a second set
/// of pipelines exporting the same measurements twice.
#[derive(Debug)]
pub struct Exports {
    tracer: SdkTracerProvider,
    cumulative: SdkMeterProvider,
    delta: SdkMeterProvider,
    logger: SdkLoggerProvider,
    /// How many spans this process has failed to deliver.
    spans_lost: SpanDrops,
    /// How many metric collection cycles it has failed to deliver.
    cycles_lost: BatchDrops,
}

impl Exports {
    /// Delivers everything buffered, for a process that is going away.
    ///
    /// Every signal, and failures are reported rather than raised: this runs
    /// during shutdown, where there is nothing left to abort and a lost batch
    /// is worth a line rather than a non-zero exit.
    pub fn flush(&self) {
        for (signal, outcome) in [
            ("traces", self.tracer.force_flush()),
            ("metrics", self.cumulative.force_flush()),
            ("metrics_delta", self.delta.force_flush()),
            ("logs", self.logger.force_flush()),
        ] {
            if let Err(failure) = outcome {
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

    /// The logger provider, for the bridge that feeds it log records.
    #[must_use]
    pub const fn logger(&self) -> &SdkLoggerProvider {
        &self.logger
    }

    /// The tracer provider, for the bridge that feeds it spans.
    #[must_use]
    pub const fn tracer(&self) -> &SdkTracerProvider {
        &self.tracer
    }

    /// Spans this process failed to deliver.
    ///
    /// The number an operator acts on when a collector is unreachable, and the
    /// reason the export is allowed to fail quietly: telemetry that is lost
    /// says so, so nobody has to infer it from an empty dashboard.
    #[must_use]
    pub fn spans_lost(&self) -> &SpanDrops {
        &self.spans_lost
    }

    /// Metric collection cycles this process failed to deliver.
    ///
    /// Cycles rather than data points: losing one loses a MOMENT, not a
    /// quantity, and the next cycle carries the running total again for every
    /// cumulative family.
    #[must_use]
    pub fn cycles_lost(&self) -> &BatchDrops {
        &self.cycles_lost
    }
}

/// Builds every pipeline, installs the process-wide handles, and claims the
/// instrument set.
///
/// # Errors
///
/// A configuration the exporter will not accept — an endpoint that is not a
/// URI — or a census the instrument layer refuses. Both refuse boot: each is a
/// defect that would otherwise present as a collector receiving nothing.
pub fn install(config: &OtlpConfig, sources: &GaugeSources) -> Result<Exports, BootFailure> {
    let resource = self::resource::describe();
    let protocol = protocol_of(config);
    let headers: HashMap<String, String> = config.headers.iter().cloned().collect();

    let spans = CountingExporter::new(
        opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(signal_endpoint(config, TRACES_PATH))
            .with_protocol(protocol)
            .with_timeout(config.timeout)
            .with_headers(headers.clone())
            .build()?,
    );
    let spans_lost = spans.drops();
    let tracer = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(spans)
        .build();

    let logs = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_endpoint(signal_endpoint(config, LOGS_PATH))
        .with_protocol(protocol)
        .with_timeout(config.timeout)
        .with_headers(headers.clone())
        .build()?;
    let logger = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(logs)
        .build();

    let registry = Registry::declared()?;
    let (cumulative, cycles_lost) =
        meter_provider(config, &resource, &registry, Temporality::Cumulative, &headers)?;
    let (delta, _delta_drops) =
        meter_provider(config, &resource, &registry, Temporality::Delta, &headers)?;

    // The globals the recording side reaches through. Set BEFORE the
    // instruments are claimed, so a family built here is built on the provider
    // this process will actually export from.
    opentelemetry::global::set_tracer_provider(tracer.clone());
    opentelemetry::global::set_meter_provider(cumulative.clone());

    let instruments = Instruments::new(
        registry,
        cumulative.meter(semconv::SCOPE_NAME),
        delta.meter(semconv::SCOPE_NAME),
    );
    let installed = producers::install(&instruments, sources)?;
    announce(config, installed, &instruments);

    Ok(Exports {
        tracer,
        cumulative,
        delta,
        logger,
        spans_lost,
        // The cumulative provider's, and the delta provider keeps its own.
        // One number rather than two because the question it answers is
        // whether the collector is taking metrics at all, and both readers
        // post to the same endpoint — a run where one succeeds and the other
        // does not is a collector rejecting a temporality, which the
        // discarded-entries counter names precisely.
        cycles_lost,
    })
}

/// One meter provider, exporting at `temporality`.
fn meter_provider(
    config: &OtlpConfig,
    resource: &Resource,
    registry: &Registry,
    temporality: Temporality,
    headers: &HashMap<String, String>,
) -> Result<(SdkMeterProvider, BatchDrops), BootFailure> {
    let exporter = CountingMetricExporter::new(
        opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(signal_endpoint(config, METRICS_PATH))
            .with_protocol(protocol_of(config))
            .with_timeout(config.timeout)
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

/// Where one signal is posted.
fn signal_endpoint(config: &OtlpConfig, path: &str) -> String {
    format!("{}{path}", config.endpoint.trim_end_matches('/'))
}

/// The encoding, as the exporter's own vocabulary spells it.
fn protocol_of(config: &OtlpConfig) -> Protocol {
    if &*config.protocol == PROTOCOL_JSON {
        return Protocol::HttpJson;
    }
    Protocol::HttpBinary
}

/// Says what is exporting, where from, and what nothing feeds.
///
/// The endpoint is named by its SOURCE and never by its value: it is read from
/// the same place as the credential beside it, and a line carrying one is a
/// line a reader will assume carries neither.
fn announce(config: &OtlpConfig, installed: bool, instruments: &Instruments) {
    let source = config.source;
    let protocol = config.protocol.as_ref();
    let families = instruments.registry().len() - instruments.unclaimed().len();
    tracing::info!(
        source,
        protocol,
        families,
        installed,
        event = "startup_otel_enabled",
        "telemetry is exporting"
    );
    for row in afd_observability::metrics::produced::UNPRODUCED {
        // `debug`, and once per boot: this is a standing property of the build
        // rather than an event, and an operator reads it when a family they
        // expected is missing.
        tracing::debug!(
            family = row.family,
            reason = row.why,
            event = "metric_family_unproduced",
            "a declared family has no producer in this build"
        );
    }
}
