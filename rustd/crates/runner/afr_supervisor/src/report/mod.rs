//! A run's terminal report, built from how its run ended.

pub(crate) mod delivery;

use std::borrow::Cow;
use std::time::Duration;

use af_core::clock::saturating_millis;
use af_wire::report::{FailureClass, Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::RunPayload;
use afr_agent::{ExecutionResult, Failure, ResultOutcome};
use afr_agent::{Meter, RunOutput};
use jiff::Timestamp;

/// How a run's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ending {
    /// The engine ran the turn to its end; the result may itself be a failure
    /// the fleet caused.
    Ran {
        /// What the engine handed back.
        output: Box<RunOutput>,
        /// When the first answer chunk arrived, from the run's start.
        first_chunk: Option<Duration>,
    },
    /// The run never finished: it could not start, the engine broke, or the
    /// daemon ended the run.
    Failed {
        /// Why, at the granularity the report carries.
        class: FailureClass,
        /// Why, in a sentence an operator reads.
        detail: &'static str,
    },
    /// A person stopped the run's turn, and the daemon told this runner so.
    /// What the engine handed back, if it did, is kept for its tokens, items
    /// and memory; its answer is not, because a stopped turn gets none.
    Stopped {
        /// What the engine handed back before it stopped, when it did.
        output: Option<Box<RunOutput>>,
        /// When the first answer chunk arrived, from the run's start.
        first_chunk: Option<Duration>,
    },
}

impl Ending {
    /// Whether the run ended as the fleet meant it to: the report says
    /// `processed`, which is the one ending whose sandbox is worth holding.
    pub(crate) fn processed(&self) -> bool {
        verdict(self).0 == Outcome::Processed
    }

    /// This ending, for a run the daemon or the runner ended before the run
    /// did: a run that handed back output keeps it, with `class` and `detail`
    /// as its failure, so its tokens are billed and its memory pushed.
    /// This ending, for a run the daemon stopped: whatever the engine handed
    /// back is kept, and the report says `stopped`.
    pub(crate) fn stopped(self) -> Self {
        match self {
            Self::Ran {
                output,
                first_chunk,
            } => Self::Stopped {
                output: Some(output),
                first_chunk,
            },
            Self::Failed { .. } => Self::Stopped {
                output: None,
                first_chunk: None,
            },
            stopped @ Self::Stopped { .. } => stopped,
        }
    }

    /// What the engine handed back, on an ending that kept it.
    pub(crate) fn output_mut(&mut self) -> Option<&mut RunOutput> {
        match self {
            Self::Ran { output, .. }
            | Self::Stopped {
                output: Some(output),
                ..
            } => Some(output),
            Self::Failed { .. } | Self::Stopped { output: None, .. } => None,
        }
    }

    pub(crate) fn cut(self, class: FailureClass, detail: &'static str) -> Self {
        match self {
            Self::Ran {
                mut output,
                first_chunk,
            } => {
                // A renewal cut revokes permission to request a task transition.
                // Usage and ordinary report evidence still settle below.
                output.task = None;
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
            stopped @ Self::Stopped { .. } => stopped,
        }
    }
}

/// The report for `run`, which ran for `wall`, carrying `trace`: the run's
/// trace encoded, when it called a tool, and `held_until`: when the sandbox it
/// left held for the fleet's next run lapses, if it left one. It carries the
/// request lines no renewal recorded, at most one body's worth, since the
/// settle renews the rest away first. A run that never finished still bills
/// what it spent; its token total is the result's when the run handed one back,
/// for the event history.
pub(crate) fn report<'a>(
    run: &'a RunPayload<'a>,
    ending: &'a Ending,
    meter: &Meter,
    wall: Duration,
    trace: Option<af_wire::tool_trace::RawToolTrace<'a>>,
    held_until: Option<Timestamp>,
) -> ReportRequest<'a> {
    let (outcome, failure_reason, failure_detail) = verdict(ending);
    let (result, first_chunk) = match ending {
        Ending::Ran {
            output,
            first_chunk,
        } => (Some(&output.result), *first_chunk),
        Ending::Stopped {
            output,
            first_chunk,
        } => (output.as_ref().map(|output| &output.result), *first_chunk),
        Ending::Failed { .. } => (None, None),
    };
    // A stopped turn gets no answer, whatever the engine had written.
    let response_text = match ending {
        Ending::Ran { .. } => result.map_or(Cow::Borrowed(""), |result| {
            Cow::Borrowed(result.content.as_ref())
        }),
        Ending::Stopped { .. } | Ending::Failed { .. } => Cow::Borrowed(""),
    };
    let tokens = result.map_or_else(|| meter.read().total(), |result| result.token_count);
    ReportRequest {
        task: match ending {
            Ending::Ran { output, .. } if outcome == Outcome::Processed => output.task.clone(),
            Ending::Ran { .. } | Ending::Failed { .. } | Ending::Stopped { .. } => None,
        },
        run_id: Cow::Borrowed(&run.run_id),
        turn_id: Cow::Borrowed(&run.event.turn_id),
        fencing_token: run.fencing_token,
        outcome,
        failure_reason,
        failure_detail,
        response_text: response_text.clone(),
        tokens,
        requests: meter.unrecorded(),
        telemetry: report_telemetry(first_chunk, wall),
        checkpoint: ReportCheckpoint {
            last_turn_id: Cow::Borrowed(&run.event.turn_id),
            last_response: response_text,
        },
        tool_calls: trace,
        // The wire still spells this instant as epoch milliseconds.
        held_until,
        // Only a processed run's folds settle with its answer; any other
        // ending leaves them for the fleet's next run.
        folded_through_sequence: folded_through(ending, outcome),
        setup: None,
        memory: Vec::new(),
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
        Ending::Stopped { .. } => (Outcome::Stopped, None, Cow::Borrowed("")),
    }
}

/// A count the wire carries in 32 bits, saturated rather than wrapped.
pub(crate) fn narrow(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;

fn folded_through(ending: &Ending, outcome: Outcome) -> Option<i64> {
    match ending {
        Ending::Ran { output, .. } if outcome == Outcome::Processed => {
            output.folded_through_sequence
        }
        Ending::Ran { .. } | Ending::Failed { .. } | Ending::Stopped { .. } => None,
    }
}

fn report_telemetry(first_chunk: Option<Duration>, wall: Duration) -> ReportTelemetry {
    ReportTelemetry {
        time_to_first_token_ms: narrow(first_chunk.map_or(0, saturating_millis)),
        wall_ms: saturating_millis(wall),
        segments: None,
    }
}
