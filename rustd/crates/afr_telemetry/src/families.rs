//! The runner's own metric families: declared in their own census, claimed
//! from it once, recorded through the [`Recorder`] they implement.
//!
//! The census is `docs/metrics.runner.census.tsv`, compiled in and read by the
//! daemon's own registry reader, so the two binaries' censuses are one format
//! graded one way. Neither census declares the other's families.

use std::time::Duration;

use afd_observability::metrics::family::{CounterKind, Declared, HistogramKind};
use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::label::http::{DiscardReason, Signal};
use afd_observability::metrics::registry::Registry;
use afd_observability::semconv::{LABEL_OUTCOME, LABEL_REASON, LABEL_SIGNAL};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};

use crate::labels::{
    FrameDrop, LABEL_PROVIDER, LABEL_TOOL, Provider, PushFailure, RetryReason, SandboxStart, Tool,
    ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

#[cfg(test)]
mod tests;

/// The runner's census, compiled in so a runner can never disagree with the
/// file its producers are graded against.
pub const CENSUS: &str = include_str!("../../../../docs/metrics.runner.census.tsv");

/// Model turn wall time, by provider and outcome.
pub const PROVIDER_TURN_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_provider_turn_duration_seconds");

/// Turn sends retried, by provider and reason.
pub const PROVIDER_RETRIES: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_provider_retries_total");

/// Time to a sandbox whose executor answers, by outcome.
pub const SANDBOX_START_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_sandbox_start_duration_seconds");

/// Live-tail frames lost on the runner, by reason.
pub const ACTIVITY_FRAMES_DROPPED: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_activity_frames_dropped_total");

/// Memory pushes that did not land, by reason.
pub const MEMORY_PUSH_FAILURES: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_memory_push_failures_total");

/// Tool call wall time, by tool and outcome.
pub const TOOL_CALL_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_tool_call_duration_seconds");

/// Tenant processes the kernel killed for memory inside a sandbox.
pub const TOOL_OUT_OF_MEMORY: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_tool_out_of_memory_total");

/// Spans the span budget shed.
pub const SPANS_SUPPRESSED: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_spans_suppressed_total");

/// Telemetry the export lost before the collector took it, by signal and
/// reason.
pub const OTLP_ENTRIES_DISCARDED: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_otlp_entries_discarded_total");

/// Reads the runner's census.
///
/// # Errors
/// A row the reader or the closed vocabularies reject.
pub fn registry() -> afd_observability::Result<Registry> {
    Registry::read(CENSUS)
}

/// Every runner instrument, claimed once, recording into the export.
#[derive(Debug)]
pub struct Families {
    turns: Histogram<f64>,
    retries: Counter<u64>,
    sandbox_starts: Histogram<f64>,
    frames_dropped: Counter<u64>,
    push_failures: Counter<u64>,
    tool_calls: Histogram<f64>,
    out_of_memory: Counter<u64>,
    spans_suppressed: Counter<u64>,
    entries_discarded: Counter<u64>,
}

impl Families {
    /// Claims every runner family from `instruments`.
    ///
    /// # Errors
    /// A family the census does not declare, or declares as another kind or
    /// number: the code and the census were edited apart.
    pub fn claim(instruments: &Instruments) -> afd_observability::Result<Self> {
        Ok(Self {
            turns: instruments.histogram_f64(&PROVIDER_TURN_DURATION)?,
            retries: instruments.counter_u64(&PROVIDER_RETRIES)?,
            sandbox_starts: instruments.histogram_f64(&SANDBOX_START_DURATION)?,
            frames_dropped: instruments.counter_u64(&ACTIVITY_FRAMES_DROPPED)?,
            push_failures: instruments.counter_u64(&MEMORY_PUSH_FAILURES)?,
            tool_calls: instruments.histogram_f64(&TOOL_CALL_DURATION)?,
            out_of_memory: instruments.counter_u64(&TOOL_OUT_OF_MEMORY)?,
            spans_suppressed: instruments.counter_u64(&SPANS_SUPPRESSED)?,
            entries_discarded: instruments.counter_u64(&OTLP_ENTRIES_DISCARDED)?,
        })
    }
}

impl Recorder for Families {
    fn turn(&self, provider: Provider, outcome: TurnOutcome, elapsed: Duration) {
        self.turns.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new(LABEL_PROVIDER, provider.as_str()),
                KeyValue::new(LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }

    fn retry(&self, provider: Provider, reason: RetryReason) {
        self.retries.add(
            1,
            &[
                KeyValue::new(LABEL_PROVIDER, provider.as_str()),
                KeyValue::new(LABEL_REASON, reason.as_str()),
            ],
        );
    }

    fn sandbox_start(&self, outcome: SandboxStart, elapsed: Duration) {
        self.sandbox_starts.record(
            elapsed.as_secs_f64(),
            &[KeyValue::new(LABEL_OUTCOME, outcome.as_str())],
        );
    }

    fn frames_dropped(&self, reason: FrameDrop, frames: u64) {
        self.frames_dropped
            .add(frames, &[KeyValue::new(LABEL_REASON, reason.as_str())]);
    }

    fn push_failed(&self, reason: PushFailure) {
        self.push_failures
            .add(1, &[KeyValue::new(LABEL_REASON, reason.as_str())]);
    }

    fn tool_call(&self, tool: Tool, outcome: ToolOutcome, elapsed: Duration) {
        self.tool_calls.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new(LABEL_TOOL, tool.as_str()),
                KeyValue::new(LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }

    fn out_of_memory(&self) {
        self.out_of_memory.add(1, &[]);
    }

    fn spans_suppressed(&self, spans: u64) {
        self.spans_suppressed.add(spans, &[]);
    }

    fn export_discarded(&self, signal: Signal, reason: DiscardReason, count: u64) {
        self.entries_discarded.add(
            count,
            &[
                KeyValue::new(LABEL_SIGNAL, signal.as_str()),
                KeyValue::new(LABEL_REASON, reason.as_str()),
            ],
        );
    }
}
