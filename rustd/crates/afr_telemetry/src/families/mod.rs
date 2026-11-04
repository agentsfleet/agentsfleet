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

use crate::labels::decision::{DecisionOutcome, DecisionProvider, DecisionPurpose, LABEL_PURPOSE};
use crate::labels::{
    FrameDrop, LABEL_PROVIDER, LABEL_TOOL, Provider, PushFailure, RetryReason, SandboxBuild,
    SandboxHold, SandboxRelease, SandboxStart, SandboxWait, Tool, ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

#[cfg(test)]
mod tests;

/// The runner's census, compiled in so a runner can never disagree with the
/// file its producers are graded against.
pub const CENSUS: &str = include_str!("../../../../../docs/metrics.runner.census.tsv");

/// Individual provider stream duration, attributed to a catalogue model.
pub const MODEL_CALL_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_model_call_duration_seconds");
/// First nonempty text for a provider stream, absent if it produced none.
pub const MODEL_FIRST_TOKEN: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_model_first_token_duration_seconds");

/// Model turn wall time, by provider and outcome.
pub const PROVIDER_TURN_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_provider_turn_duration_seconds");

/// Turn sends retried, by provider and reason.
pub const PROVIDER_RETRIES: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_provider_retries_total");

/// Time to a sandbox whose executor answers, by outcome.
pub const SANDBOX_START_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_sandbox_start_duration_seconds");

/// How long a sandbox tool call waited for its lease's build, by outcome.
pub const SANDBOX_WAIT_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_sandbox_wait_duration_seconds");

/// Time to release a sandbox's disk after its report, by outcome.
pub const SANDBOX_RELEASE_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_sandbox_release_duration_seconds");

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

/// Lease sandbox builds, by how each ended.
pub const SANDBOX_BUILDS: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_sandbox_builds_total");

/// Sandboxes held between a fleet's leases, by what became of them.
pub const SANDBOX_HOLDS: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_sandbox_holds_total");

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
    decisions: Counter<u64>,
    decision_duration: Histogram<f64>,
    turns: Histogram<f64>,
    memory_sources: Counter<u64>,
    model_calls: Histogram<f64>,
    model_tokens: Counter<u64>,
    model_cache_read: Counter<u64>,
    model_cache_write: Counter<u64>,
    model_first: Histogram<f64>,
    retries: Counter<u64>,
    sandbox_starts: Histogram<f64>,
    sandbox_waits: Histogram<f64>,
    sandbox_releases: Histogram<f64>,
    sandbox_builds: Counter<u64>,
    frames_dropped: Counter<u64>,
    push_failures: Counter<u64>,
    tool_calls: Histogram<f64>,
    out_of_memory: Counter<u64>,
    sandbox_holds: Counter<u64>,
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
            decisions: instruments.counter_u64(&DECISION_EVALUATIONS)?,
            decision_duration: instruments.histogram_f64(&DECISION_DURATION)?,
            turns: instruments.histogram_f64(&PROVIDER_TURN_DURATION)?,
            memory_sources: instruments.counter_u64(&MEMORY_SOURCES)?,
            model_calls: instruments.histogram_f64(&MODEL_CALL_DURATION)?,
            model_tokens: instruments.counter_u64(&MODEL_TOKENS)?,
            model_cache_read: instruments.counter_u64(&MODEL_CACHE_READ)?,
            model_cache_write: instruments.counter_u64(&MODEL_CACHE_WRITE)?,
            model_first: instruments.histogram_f64(&MODEL_FIRST_TOKEN)?,
            retries: instruments.counter_u64(&PROVIDER_RETRIES)?,
            sandbox_starts: instruments.histogram_f64(&SANDBOX_START_DURATION)?,
            sandbox_waits: instruments.histogram_f64(&SANDBOX_WAIT_DURATION)?,
            sandbox_releases: instruments.histogram_f64(&SANDBOX_RELEASE_DURATION)?,
            sandbox_builds: instruments.counter_u64(&SANDBOX_BUILDS)?,
            frames_dropped: instruments.counter_u64(&ACTIVITY_FRAMES_DROPPED)?,
            push_failures: instruments.counter_u64(&MEMORY_PUSH_FAILURES)?,
            tool_calls: instruments.histogram_f64(&TOOL_CALL_DURATION)?,
            out_of_memory: instruments.counter_u64(&TOOL_OUT_OF_MEMORY)?,
            sandbox_holds: instruments.counter_u64(&SANDBOX_HOLDS)?,
            spans_suppressed: instruments.counter_u64(&SPANS_SUPPRESSED)?,
            entries_discarded: instruments.counter_u64(&OTLP_ENTRIES_DISCARDED)?,
        })
    }
}

/// Native decision evaluations, including denied preflight and cancellation.
pub const DECISION_EVALUATIONS: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_decision_evaluations_total");
/// Total bounded evaluation time, independently of coding-model turns.
pub const DECISION_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_decision_duration_seconds");

impl Recorder for Families {
    fn decision(
        &self,
        provider: DecisionProvider,
        purpose: DecisionPurpose,
        outcome: DecisionOutcome,
        elapsed: Duration,
    ) {
        let labels = [
            KeyValue::new(LABEL_PROVIDER, provider.as_str()),
            KeyValue::new(LABEL_PURPOSE, purpose.as_str()),
            KeyValue::new(LABEL_OUTCOME, outcome.as_str()),
        ];
        self.decisions.add(1, &labels);
        self.decision_duration
            .record(elapsed.as_secs_f64(), &labels);
    }

    fn memory_source(&self, source: crate::labels::MemorySource) {
        self.memory_sources.add(
            1,
            &[KeyValue::new(
                crate::labels::LABEL_MEMORY_SOURCE,
                source.as_str(),
            )],
        );
    }

    fn model_tokens(
        &self,
        model: &str,
        provider: Provider,
        usage: afd_wire::usage::ObservedTokens,
    ) {
        use afd_observability::semconv::{ATTR_PROVIDER_NAME, ATTR_REQUEST_MODEL, ATTR_TOKEN_TYPE};
        let mut base = vec![KeyValue::new(ATTR_REQUEST_MODEL, model.to_owned())];
        if let Some(name) = afd_observability::semconv::provider::normalize(provider.as_str()) {
            base.push(KeyValue::new(ATTR_PROVIDER_NAME, name));
        }
        for (kind, count) in [("input", usage.input), ("output", usage.output)] {
            if let Some(count) = count {
                let mut labels = base.clone();
                labels.push(KeyValue::new(ATTR_TOKEN_TYPE, kind));
                self.model_tokens.add(count, &labels);
            }
        }
        if let Some(count) = usage.cache_read {
            self.model_cache_read.add(count, &base);
        }
        if let Some(count) = usage.cache_write {
            self.model_cache_write.add(count, &base);
        }
    }

    fn model_call(
        &self,
        model: &str,
        provider: Provider,
        outcome: crate::labels::CallOutcome,
        elapsed: Duration,
        first_text: Option<Duration>,
    ) {
        use afd_observability::semconv::{ATTR_PROVIDER_NAME, ATTR_REQUEST_MODEL};
        let mut labels = vec![
            KeyValue::new(ATTR_REQUEST_MODEL, model.to_owned()),
            KeyValue::new(LABEL_OUTCOME, outcome.as_str()),
        ];
        if let Some(name) = afd_observability::semconv::provider::normalize(provider.as_str()) {
            labels.push(KeyValue::new(ATTR_PROVIDER_NAME, name));
        }
        self.model_calls.record(elapsed.as_secs_f64(), &labels);
        if let Some(first) = first_text {
            self.model_first.record(first.as_secs_f64(), &labels);
        }
    }

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

    fn sandbox_wait(&self, outcome: SandboxWait, elapsed: Duration) {
        self.sandbox_waits.record(
            elapsed.as_secs_f64(),
            &[KeyValue::new(LABEL_OUTCOME, outcome.as_str())],
        );
    }

    fn sandbox_release(&self, outcome: SandboxRelease, elapsed: Duration) {
        self.sandbox_releases.record(
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

    fn sandbox_hold(&self, outcome: SandboxHold) {
        self.sandbox_holds
            .add(1, &[KeyValue::new(LABEL_OUTCOME, outcome.as_str())]);
    }

    fn sandbox_build(&self, outcome: SandboxBuild) {
        self.sandbox_builds
            .add(1, &[KeyValue::new(LABEL_OUTCOME, outcome.as_str())]);
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

/// Provider-reported prompt and completion tokens.
pub const MODEL_TOKENS: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_model_tokens_total");
/// Cache-read subset of prompt tokens.
pub const MODEL_CACHE_READ: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_model_cache_read_tokens_total");
/// Cache-write subset of prompt tokens.
pub const MODEL_CACHE_WRITE: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_model_cache_write_tokens_total");

/// Inline windows and hydration fallback choices, counted once per working lease.
pub const MEMORY_SOURCES: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_memory_sources_total");
