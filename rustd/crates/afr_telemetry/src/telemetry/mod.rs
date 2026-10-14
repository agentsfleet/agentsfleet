//! The runner's pipeline, assembled: the transport, the span budget, the
//! families, and the layer that feeds them the runner's spans.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

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

/// What `close` logs when the pipelines outran its budget.
const EVENT_CLOSE_TIMED_OUT: &str = "telemetry_close_timed_out";

/// The thread `close` shuts the pipelines down on.
const CLOSING_THREAD: &str = "telemetry-close";

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
    /// The encoding on the wire, in the knob's spelling.
    protocol: &'static str,
    /// How long [`Telemetry::close`] waits: one export's allowance.
    close_within: Duration,
    /// Whether this pipeline's families are the process recorder.
    recording: bool,
}

impl Telemetry {
    /// Builds the span and metric pipelines for `endpoint`, behind the span
    /// budget; installs the runner's families as the process recorder, and
    /// routes the export's own losses to them.
    ///
    /// No log pipeline: the runner's records stay on stderr, where the runner
    /// collector reads them from the host's log store.
    ///
    /// The recorder is a process-wide slot and the first install keeps it.
    /// `run` installs once; a second install in one process exports its spans
    /// but records no family, and [`Telemetry::recording`] says so.
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
            .with_span_end(sampler.reaper())
            .install()?;
        let recording = record::install(Arc::new(Families::claim(&instruments)?));
        let _routed = record::route_export_losses();
        Ok(Self {
            exports,
            protocol: config.encoding().as_str(),
            close_within: config.timeout(),
            recording,
        })
    }

    /// Whether this pipeline's families are the ones the process records
    /// into: false only for a second install in one process.
    #[must_use]
    pub const fn recording(&self) -> bool {
        self.recording
    }

    /// The layer that exports the runner's spans: `runner.lease`,
    /// `invoke_agent`, `chat` and `execute_tool`, and nothing else.
    ///
    /// Filtered to the runner's own span target, so a library's span never
    /// leaves the host, and to spans alone, so no log record rides a span as
    /// an event: the four constructors in `afr_agent` and `afr_supervisor` are
    /// the only things that decide what a runner span carries, and none takes
    /// a prompt, a reply or a tool's output. The tool span names its tool by
    /// the catalog's closed set, never the model's own spelling.
    ///
    /// Context activation, inactivity timing and the target attribute are off.
    /// The runner's spans wrap long futures — a lease, a run, a streamed turn
    /// — so activation would swap the OpenTelemetry context and take two locks
    /// on every poll, for a context nothing in the runner reads; a child's
    /// parent is found from the span stack either way, so a lease's root still
    /// reaches the sampler first. The target is the filter's own constant.
    #[must_use]
    pub fn layer(&self) -> SpanLayer {
        let tracer = self.exports.tracer().tracer(RUNNER_SCOPE_NAME);
        Box::new(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_location(false)
                .with_threads(false)
                .with_target(false)
                .with_context_activation(false)
                .with_tracked_inactivity(false)
                .with_filter(filter_fn(is_runner_span)),
        )
    }

    /// Says the runner is exporting, by the knob's name and never its value.
    pub fn announce(&self) {
        let knob = OTEL_ENDPOINT_KNOB;
        let protocol = self.protocol;
        let event = EVENT_STARTED;
        tracing::info!(knob, protocol, event);
    }

    /// Delivers what the pipelines hold and keeps them running. Parks the
    /// calling thread; a caller on a reactor moves it to the blocking pool.
    pub fn flush(&self) {
        self.exports.flush();
    }

    /// Delivers what the pipelines hold and stops them, for a runner that is
    /// exiting, waiting no longer than one export's allowance.
    ///
    /// One round of exports on the way out: the shutdown delivers, and the
    /// providers' own `Drop` then finds nothing to do.
    pub fn close(self) {
        let within = self.close_within;
        self.close_within(within);
    }

    /// [`Telemetry::close`], waiting at most `budget`.
    ///
    /// The shutdown runs on a thread of its own. Past the budget the runner
    /// stops waiting and says so: a collector that never answers must not
    /// hold a runner's exit, and what was still queued is lost either way.
    pub fn close_within(self, budget: Duration) {
        let (done, finished) = mpsc::channel();
        let exports = Arc::new(self.exports);
        let closing = Arc::clone(&exports);
        let spawned = std::thread::Builder::new()
            .name(CLOSING_THREAD.into())
            .spawn(move || {
                closing.shutdown();
                let _waiting = done.send(());
            });
        if spawned.is_err() {
            // No thread to bound it on: shut down here, unbounded, rather
            // than leave the span layer's provider clone holding what is
            // buffered past the process's exit.
            exports.shutdown();
            return;
        }
        if finished.recv_timeout(budget).is_err() {
            let budget_ms = budget.as_millis();
            let event = EVENT_CLOSE_TIMED_OUT;
            tracing::warn!(budget_ms, event, "some telemetry was not delivered");
        }
    }

    /// The pipelines, for a test that reads their losses. A runner reports
    /// them through `agentsfleet_runner_otlp_entries_discarded_total` instead.
    #[cfg(test)]
    pub(crate) const fn exports(&self) -> &Exports {
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
