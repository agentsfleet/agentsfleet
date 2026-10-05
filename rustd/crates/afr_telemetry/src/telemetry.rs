//! The runner's pipeline, assembled: the transport, the span budget, the
//! families, and the layer that feeds them the runner's spans.

use std::sync::Arc;

use afd_observability::semconv::RUNNER_SCOPE_NAME;
use afd_otlp::{Builder, Exports, OTEL_ENDPOINT_KNOB, Service};
use opentelemetry::trace::TracerProvider as _;
use tracing::Metadata;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::{Layer as _, Registry};

use crate::budget::{LeaseSampler, Limits, Monotonic};
use crate::endpoint::Endpoint;
use crate::error::Result;
use crate::families::{self, Families};
use crate::record;

#[cfg(test)]
mod tests;

/// What `run` logs when it exports.
const EVENT_STARTED: &str = "telemetry_export_started";

/// What `run` logs when it does not.
const EVENT_DISABLED: &str = "telemetry_export_disabled";

/// The runner, as every signal it sends describes it.
const SERVICE: Service = Service::new(RUNNER_SCOPE_NAME, env!("CARGO_PKG_VERSION"));

/// The layer the runner's subscriber carries to export its spans.
pub type SpanLayer = Box<dyn tracing_subscriber::Layer<Registry> + Send + Sync>;

/// The runner's export, once `run` has an endpoint.
///
/// Not `Clone`: one per process, as there is one pipeline per process.
#[derive(Debug)]
pub struct Telemetry {
    exports: Exports,
    /// The knob the endpoint came from, for the line that reports it.
    knob: &'static str,
    /// The encoding on the wire, in the knob's spelling.
    protocol: &'static str,
}

impl Telemetry {
    /// Builds the span and metric pipelines for `endpoint`, behind the span
    /// budget, and installs the runner's families as the process recorder.
    ///
    /// No log pipeline: the runner's records stay on stderr, where the runner
    /// collector reads them from the host's log store.
    ///
    /// # Errors
    /// A census the instrument layer refuses, or a transport that will not
    /// build from the accepted endpoint.
    pub fn install(endpoint: &Endpoint) -> Result<Self> {
        let sampler = LeaseSampler::new(Limits::default(), Monotonic::start(), || {
            record::spans_suppressed(1);
        });
        let config = endpoint.config();
        let (exports, instruments) = Builder::new(config, SERVICE, families::registry()?)
            .with_sampler(sampler.clone())
            .with_span_processor(sampler.reaper())
            .install()?;
        let _first = record::install(Arc::new(Families::claim(&instruments)?));
        Ok(Self {
            exports,
            knob: config.source(),
            protocol: config.encoding().as_str(),
        })
    }

    /// The layer that exports the runner's spans: `runner.lease`,
    /// `invoke_agent`, `chat` and `execute_tool`, and nothing else.
    ///
    /// Filtered to the runner's own span target, so a library's span never
    /// leaves the host, and to spans alone, so no log record rides a span as
    /// an event: the four constructors in `afr_agent` and `afr_supervisor` are
    /// the only things that decide what a runner span carries, and none takes
    /// a prompt, a reply or a tool's output.
    #[must_use]
    pub fn layer(&self) -> SpanLayer {
        let tracer = self.exports.tracer().tracer(RUNNER_SCOPE_NAME);
        Box::new(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_location(false)
                .with_threads(false)
                .with_filter(filter_fn(is_runner_span)),
        )
    }

    /// Says the runner is exporting, by the knob's name and never its value.
    pub fn announce(&self) {
        let knob = self.knob;
        let protocol = self.protocol;
        let event = EVENT_STARTED;
        tracing::info!(knob, protocol, event);
    }

    /// Delivers what the pipelines hold, for a runner that is stopping.
    /// Parks the calling thread; a caller on a reactor moves it to the
    /// blocking pool.
    pub fn flush(&self) {
        self.exports.flush();
    }

    /// The pipelines, for a caller that reports their losses.
    #[must_use]
    pub const fn exports(&self) -> &Exports {
        &self.exports
    }
}

/// Says the runner is not exporting, naming the knob that would turn it on.
pub fn announce_disabled() {
    let knob = OTEL_ENDPOINT_KNOB;
    let event = EVENT_DISABLED;
    tracing::info!(knob, event);
}

/// Whether `metadata` is one of the runner's own spans.
fn is_runner_span(metadata: &Metadata<'_>) -> bool {
    metadata.is_span() && metadata.target() == RUNNER_SCOPE_NAME
}
