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

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::registry::Registry;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::Temporality;
use opentelemetry_sdk::trace::{ShouldSample, SpanData};

use crate::config::OtlpConfig;
use crate::error::Result;
use crate::resource::Service;

mod exports;
mod providers;

pub use self::exports::Exports;
use self::providers::{logger, meter_provider, tracer};

#[cfg(test)]
mod tests;

/// The signal path each exporter posts under.
pub(crate) const TRACES_PATH: &str = "/v1/traces";
pub(crate) const METRICS_PATH: &str = "/v1/metrics";
pub(crate) const LOGS_PATH: &str = "/v1/logs";

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

/// How often the metric reader collects and exports: the span batch's cadence,
/// so both signals reach the collector on one clock.
pub const COLLECT_INTERVAL: Duration = Duration::from_secs(5);

/// Sees each recorded span the moment it ends, before the batch exporter
/// takes it.
///
/// An observer rather than a second
/// [`SpanProcessor`](opentelemetry_sdk::trace::SpanProcessor): with one processor the
/// SDK MOVES each ended span into it, and with two it deep-clones the span for
/// each — every attribute string a fresh allocation, on every exported span.
/// An observer reads the span by reference inside the one processor, so the
/// span is moved once whatever is watching.
pub trait SpanEnd: Send + Sync + core::fmt::Debug {
    /// `span` ended and was recorded; it goes to the exporter next.
    fn ended(&self, span: &SpanData);
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
