//! A lease's terminal report, built from how its run ended.

use std::borrow::Cow;
use std::time::Duration;

use afd_wire::lease::LeasePayload;
use afd_wire::report::{
    ExecutionResult, FailureClass, Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry,
    ResultOutcome,
};
use afr_agent::RunOutput;

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

/// The report for `lease`, which ran for `wall`.
pub(crate) fn report<'a>(
    lease: &'a LeasePayload<'a>,
    ending: &'a Ending,
    wall: Duration,
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
    ReportRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        event_id: Cow::Borrowed(&lease.event.event_id),
        fencing_token: lease.fencing_token,
        outcome,
        failure_reason,
        failure_detail,
        response_text: response_text.clone(),
        tokens: result.map_or(0, |result| result.token_count),
        input_tokens: narrow(result.map_or(0, |result| result.input_tokens)),
        cached_input_tokens: narrow(result.map_or(0, |result| result.cached_input_tokens)),
        output_tokens: narrow(result.map_or(0, |result| result.output_tokens)),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: narrow(first_chunk.map_or(0, millis)),
            wall_ms: millis(wall),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event.event_id),
            last_response: response_text,
        },
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
fn narrow(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// A duration in whole milliseconds, saturated.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "report/tests.rs"]
mod tests;
