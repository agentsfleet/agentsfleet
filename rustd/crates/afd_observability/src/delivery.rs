//! The span a finished run leaves behind, built backwards from its end.
//!
//! # Why this span is SYNTHESIZED rather than scoped
//!
//! Every other span in this daemon wraps work it is watching: the scope opens,
//! the work happens, the scope closes. This one cannot. The work happened in a
//! sandbox on another host, and all that reaches here is a report saying how
//! long it took. So the span is constructed after the fact — its end is the
//! moment the report settled, and its start is that moment less the duration
//! the runner reported.
//!
//! A `#[instrument]` on the report handler would be the wrong shape and a
//! quietly wrong number: it would time the HTTP call that DELIVERED the
//! result, which is milliseconds, rather than the run, which is minutes.
//!
//! # Why it is a root rather than a child of the request
//!
//! The request that carries the report is not the parent of the run — it
//! arrives after the run has already finished. Making it one would produce a
//! child whose start precedes its parent's by the whole length of the run,
//! which every trace backend reads as a clock fault. The Zig daemon reaches
//! the same place from the other side: it generates a fresh trace context
//! because it has no parent available at all.
//!
//! # Nothing here reads a payload
//!
//! Prompt text, response bodies and credentials are absent by construction:
//! the attributes below are identifiers, counts and closed labels, and there
//! is no field on this type that could carry anything else.

use std::time::{Duration, SystemTime};

use opentelemetry::trace::{Span as _, SpanBuilder, SpanKind, Tracer};
use opentelemetry::{Context, KeyValue};

use crate::metrics::label::http::{OmissionReason, OmittedAttribute};
use crate::semconv;

#[cfg(test)]
mod tests;

/// The longest run this daemon will describe as one span: one week.
///
/// The duration arrives from a runner and is therefore not trusted. Without a
/// cap, a wrong reading does not produce a wrong span — it produces a span
/// whose start is years in the past, which drags a whole trace's timeline with
/// it and cannot be filtered out by anyone reading the backend afterwards.
/// Beyond this, the run is described as this long and the report is still
/// recorded; the alternative, discarding it, loses the delivery entirely over
/// one bad field.
pub const MAX_RUN: Duration = Duration::from_hours(HOURS_IN_A_WEEK);

/// One week, in the unit the constructor takes.
///
/// Hours rather than seconds because `604_800` is a number a reader has to
/// divide twice before it means anything, and `Duration::from_weeks` is not
/// stable on the pinned toolchain.
const HOURS_IN_A_WEEK: u64 = 24 * 7;

/// A settled run, as the span it becomes.
///
/// Borrowed rather than owned throughout: every field is read out of the lease
/// row the report already loaded, and the span is built and finished inside
/// one call. Owning them would copy a run's whole identity to describe it.
#[derive(Debug)]
pub struct Delivery<'a> {
    /// The tenant whose wallet the run drew on.
    pub tenant_id: &'a str,
    /// The workspace the run belongs to.
    pub workspace_id: &'a str,
    /// The fleet that ran, which is this product's agent identity.
    pub fleet_id: &'a str,
    /// The event the run executed.
    pub event_id: &'a str,
    /// The sandbox posture, in the spelling the lease row stored.
    pub posture: &'a str,
    /// The provider the run was issued against, as configured.
    pub provider: &'a str,
    /// The model the run was issued against.
    pub model: &'a str,
    /// Prompt tokens for the whole run, cached input included.
    pub input_tokens: u64,
    /// Completion tokens for the whole run.
    pub output_tokens: u64,
    /// How long the runner says the run took.
    pub wall: Duration,
}

impl Delivery<'_> {
    /// Records this delivery as one finished span on the installed tracer.
    ///
    /// `completed_at` is when the report settled — the span's end, and the
    /// point its start is measured back from. Passed in rather than read from
    /// a clock here so the caller's one reading of the wall clock is the one
    /// every record of this delivery agrees on.
    ///
    /// Best-effort by construction, twice over: a deployment that configured
    /// no exporter has the API's no-op tracer installed, and this costs an
    /// allocation of attributes nobody reads. Nothing about a report's outcome
    /// depends on it, which is why it answers nothing.
    ///
    /// The tracer is reached through the process-wide registry rather than
    /// threaded down from boot. That is what keeps a report handler free of
    /// any telemetry seam to hold, forget, or stub — and the registry is the
    /// SDK's own integration point, not a global this crate invented.
    pub fn record(&self, completed_at: SystemTime) {
        self.record_on(
            &opentelemetry::global::tracer(semconv::SCOPE_NAME),
            completed_at,
        );
    }

    /// The same recording, against a tracer the caller names.
    ///
    /// Private, and the tests are inside this module for that reason: what
    /// belongs to the rest of the daemon is one way to record a delivery, and
    /// a second public entry point taking a tracer would be an invitation to
    /// build a second pipeline beside the installed one.
    fn record_on<T: Tracer>(&self, tracer: &T, completed_at: SystemTime) {
        let Some(started_at) = self.started_at(completed_at) else {
            // `warn` rather than `debug`: a run this daemon cannot place in
            // time is one an operator will look for and not find.
            let fleet_id = self.fleet_id;
            let wall_ms = self.wall.as_millis();
            tracing::warn!(
                fleet_id,
                wall_ms,
                reason = "unplaceable_start",
                event = "skip_delivery_span",
                "the reported duration puts this run's start before the epoch"
            );
            return;
        };

        // An empty context, so the span is a ROOT — see the module note. The
        // report handler is inside the server span, and inheriting it is what
        // would make this a child that began before its parent.
        let mut span = SpanBuilder::from_name(semconv::SPAN_FLEET_DELIVERY)
            .with_kind(SpanKind::Internal)
            .with_start_time(started_at)
            .with_attributes(self.attributes())
            .start_with_context(tracer, &Context::new());
        span.end_with_timestamp(completed_at);
    }

    /// When the run began, or nothing when that is not a moment.
    ///
    /// Two ways it is not. The subtraction can leave the range `SystemTime`
    /// represents at all, and — the case that actually happens — it can land
    /// BEFORE the Unix epoch, which `SystemTime` holds perfectly well and OTLP
    /// cannot carry: a span timestamp on the wire is unsigned nanoseconds
    /// since the epoch. A start it cannot encode would be exported as some
    /// other time entirely, so it is refused here instead.
    fn started_at(&self, completed_at: SystemTime) -> Option<SystemTime> {
        let started_at = completed_at.checked_sub(self.wall.min(MAX_RUN))?;
        started_at.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        Some(started_at)
    }

    /// Everything the span says about this run.
    ///
    /// The provider is the one attribute that may be absent, and the absence
    /// is the point: a vendor with no well-known spelling is omitted rather
    /// than exported under a standard key it does not belong to.
    fn attributes(&self) -> Vec<KeyValue> {
        let mut attributes = Vec::with_capacity(semconv::DELIVERY_SPAN_KEYS.len());
        attributes.push(KeyValue::new(
            semconv::ATTR_OPERATION_NAME,
            semconv::OPERATION_INVOKE_AGENT,
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_AGENT_ID,
            self.fleet_id.to_owned(),
        ));
        match semconv::provider::normalize(self.provider) {
            Some(known) => attributes.push(KeyValue::new(semconv::ATTR_PROVIDER_NAME, known)),
            // Counted, not silent. A provider that stops being attributed
            // looks on a dashboard exactly like one nobody used.
            None => crate::producers::http::attribute_omitted(
                OmittedAttribute::ProviderName,
                OmissionReason::UnmappedProvider,
            ),
        }
        attributes.push(KeyValue::new(
            semconv::ATTR_REQUEST_MODEL,
            self.model.to_owned(),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_USAGE_INPUT_TOKENS,
            saturating_signed(self.input_tokens),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_USAGE_OUTPUT_TOKENS,
            saturating_signed(self.output_tokens),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_EXECUTION_POSTURE,
            self.posture.to_owned(),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_WORKSPACE_ID,
            self.workspace_id.to_owned(),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_TENANT_ID,
            self.tenant_id.to_owned(),
        ));
        attributes.push(KeyValue::new(
            semconv::ATTR_EVENT_ID,
            self.event_id.to_owned(),
        ));
        attributes
    }
}

/// A runner-reported count as the signed integer an attribute holds.
///
/// Saturating rather than wrapping or refusing: the counts are runner-supplied
/// and an absurd one is a telemetry defect, not a reason to lose the span that
/// carries the other nine attributes.
fn saturating_signed(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}
