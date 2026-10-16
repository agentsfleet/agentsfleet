//! What one process's telemetry owns once it is built, and the flush and
//! shutdown that hand its buffers to the collector on the way out.
//!
//! Split from the module root: the root builds the pipelines, this holds them.

use afd_observability::metrics::export::BatchDrops;
use afd_observability::{LogDrops, SpanDrops};
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// The signal a flush failure names, by the pipeline it came from.
const SIGNAL_TRACES: &str = "traces";
const SIGNAL_METRICS: &str = "metrics";
const SIGNAL_METRICS_DELTA: &str = "metrics_delta";
const SIGNAL_LOGS: &str = "logs";

/// Everything the transport owns, held so shutdown can flush it.
///
/// Not `Clone`: there is one per process, and a second would be a second set
/// of pipelines exporting the same measurements twice.
#[derive(Debug)]
pub struct Exports {
    pub(super) tracer: SdkTracerProvider,
    pub(super) cumulative: SdkMeterProvider,
    pub(super) delta: SdkMeterProvider,
    /// The log pipeline, where the binary asked for one.
    pub(super) logger: Option<SdkLoggerProvider>,
    pub(super) spans_lost: SpanDrops,
    pub(super) cycles_lost: BatchDrops,
    pub(super) records_lost: LogDrops,
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
