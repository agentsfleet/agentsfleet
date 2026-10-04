//! A lease's terminal report, built from how its run ended.

use std::borrow::Cow;
use std::time::Duration;

use afd_core::clock::saturating_millis;
use afd_wire::lease::LeasePayload;
use afd_wire::report::{
    ExecutionResult, Failure, FailureClass, Outcome, ReportCheckpoint, ReportRequest,
    ReportTelemetry, ResultOutcome,
};
use afr_agent::{Meter, RunOutput};

/// How a lease's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ending {
    /// The engine ran the turn to its end; the result may itself be a failure
    /// the fleet caused.
    Ran {
        /// What the engine handed back.
        output: RunOutput,
        /// When the first answer chunk arrived, from the run's start.
        first_chunk: Option<Duration>,
    },
    /// The run never finished: it could not start, the engine broke, or the
    /// daemon ended the lease.
    Failed {
        /// Why, at the granularity the report carries.
        class: FailureClass,
        /// Why, in a sentence an operator reads.
        detail: &'static str,
    },
}

impl Ending {
    /// This ending, for a lease the daemon or the runner ended before the run
    /// did: a run that handed back output keeps it, with `class` and `detail`
    /// as its failure, so its tokens are billed and its memory pushed.
    pub(crate) fn cut(self, class: FailureClass, detail: &'static str) -> Self {
        match self {
            Self::Ran {
                mut output,
                first_chunk,
            } => {
                output.result.outcome = ResultOutcome::Failed(Failure {
                    class: Some(class),
                    detail: detail.into(),
                });
                Self::Ran {
                    output,
                    first_chunk,
                }
            }
            Self::Failed { .. } => Self::Failed { class, detail },
        }
    }
}

/// The report for `lease`, which ran for `wall`, carrying `trace`: the run's
/// trace encoded, when it called a tool. Its tokens are the result's when the
/// run handed one back, and otherwise what `meter` counted turn by turn, so a
/// run that never finished still bills what it spent.
pub(crate) fn report<'a>(
    lease: &'a LeasePayload<'a>,
    ending: &'a Ending,
    meter: &Meter,
    wall: Duration,
    trace: Option<&'a str>,
) -> ReportRequest<'a> {
    let (outcome, failure_reason, failure_detail) = verdict(ending);
    let (result, first_chunk) = match ending {
        Ending::Ran {
            output,
            first_chunk,
        } => (Some(&output.result), *first_chunk),
        Ending::Failed { .. } => (None, None),
    };
    let response_text = result.map_or(Cow::Borrowed(""), |result| {
        Cow::Borrowed(result.content.as_ref())
    });
    let spent = meter.read();
    let (tokens, input, cached_input, output) = result.map_or(
        (spent.total(), spent.input, spent.cached_input, spent.output),
        |result| {
            (
                result.token_count,
                result.input_tokens,
                result.cached_input_tokens,
                result.output_tokens,
            )
        },
    );
    ReportRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        event_id: Cow::Borrowed(&lease.event.event_id),
        fencing_token: lease.fencing_token,
        outcome,
        failure_reason,
        failure_detail,
        response_text: response_text.clone(),
        tokens,
        input_tokens: narrow(input),
        cached_input_tokens: narrow(cached_input),
        output_tokens: narrow(output),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: narrow(first_chunk.map_or(0, saturating_millis)),
            wall_ms: saturating_millis(wall),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event.event_id),
            last_response: response_text,
        },
        tool_calls: trace.and_then(|encoded| serde_json::from_str(encoded).ok()),
    }
}

/// The outcome, failure class and failure sentence an ending reports.
fn verdict(ending: &Ending) -> (Outcome, Option<FailureClass>, Cow<'_, str>) {
    match ending {
        Ending::Ran { output, .. } => match &output.result {
            ExecutionResult {
                outcome: ResultOutcome::Failed(failure),
                ..
            } => (
                Outcome::FleetError,
                failure.class,
                Cow::Borrowed(failure.detail.as_ref()),
            ),
            ExecutionResult { .. } => (Outcome::Processed, None, Cow::Borrowed("")),
        },
        Ending::Failed { class, detail } => {
            (Outcome::FleetError, Some(*class), Cow::Borrowed(*detail))
        }
    }
}

/// A count the wire carries in 32 bits, saturated rather than wrapped.
pub(crate) fn narrow(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "report/tests.rs"]
mod tests;
