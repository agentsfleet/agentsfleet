//! A run whose assignment will not fit one runner frame fails at once.
//!
//! The run row is written before its frame is built, so a frame the encoder
//! refused would leave a run no runner holds: it would lapse, be reassigned
//! at the next fence and fail the same way until `EVENT_RUNS_MAX` ended it,
//! the fleet's queue stalled behind it all the while. The assignment is
//! measured first instead, and a run past [`MESSAGE_BYTES_MAX`] is reported
//! failed here the way a runner reports a run it could not finish: settled
//! under its own fence, its turn failed with
//! [`FailureClass::FrameTooLarge`] and the code
//! [`error_code::RUN_FRAME_TOO_LARGE`], nothing sent and nothing billed.
//!
//! What fills a frame is the message (`event.request_json`) and the
//! instructions, each up to the text ceiling; history is already cut to
//! `af_wire::run::HISTORY_BYTES_MAX`.

use std::borrow::Cow;

use af_core::error_code;
use af_core::problem::Problem;
use af_observability::verb_span::{ReportStage, VerbClock};
use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::report::{FailureClass, Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::{SetupReport, TaggedRun};

use super::{ConnectionServices, Message};
use crate::Result;

/// The event a run failed for its frame is logged under.
const EVENT_UNFRAMED: &str = "run_frame_too_large";

/// A run whose frame would pass [`MESSAGE_BYTES_MAX`]: what failing it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unframed {
    /// The run the assignment wrote.
    run_id: String,
    /// The turn it was assigned.
    turn_id: String,
    /// The fence the assignment raised, which the failure report echoes.
    fencing_token: u64,
    /// Whether a setup draft owns the run, whose report carries its setup part.
    draft: bool,
}

impl Unframed {
    /// `message`'s run when the frame carrying it would pass
    /// [`MESSAGE_BYTES_MAX`], measured without allocating the frame; `None`
    /// for one that fits.
    pub(super) fn of(message: &Message<'_>) -> Option<Self> {
        if af_validate::serialized_within(message, MESSAGE_BYTES_MAX) {
            return None;
        }
        let Message::Run {
            run: TaggedRun::Fleet(payload),
        } = message
        else {
            return None;
        };
        Some(Self {
            run_id: payload.run_id.clone().into_owned(),
            turn_id: payload.event.turn_id.clone().into_owned(),
            fencing_token: payload.fencing_token,
            draft: payload.setup.is_some(),
        })
    }

    /// The failure report a runner would have sent for this run, had it been
    /// handed one it could not carry: no answer, no usage, the class and the
    /// code's own sentence.
    fn report<'a>(&'a self, detail: &'a str) -> ReportRequest<'a> {
        ReportRequest {
            run_id: Cow::Borrowed(&self.run_id),
            turn_id: Cow::Borrowed(&self.turn_id),
            fencing_token: self.fencing_token,
            outcome: Outcome::FleetError,
            task: None,
            failure_reason: Some(FailureClass::FrameTooLarge),
            failure_detail: Cow::Borrowed(detail),
            response_text: Cow::Borrowed(""),
            tokens: 0,
            requests: af_wire::usage::RequestLines::default(),
            telemetry: ReportTelemetry {
                time_to_first_token_ms: 0,
                wall_ms: 0,
                segments: None,
            },
            checkpoint: ReportCheckpoint {
                last_turn_id: Cow::Borrowed(&self.turn_id),
                last_response: Cow::Borrowed(""),
            },
            tool_calls: None,
            held_until: None,
            folded_through_sequence: None,
            memory: Vec::new(),
            setup: self.draft.then_some(SetupReport { proposal: None }),
        }
    }
}

/// The failure detail an unframed run's turn stores: its code, then what the
/// problem table says to do about it.
fn detail() -> String {
    let code = error_code::RUN_FRAME_TOO_LARGE;
    format!("[{}] {}", code.as_str(), Problem::of(code).hint())
}

impl ConnectionServices {
    /// Fails `unframed`'s run and its turn at once, through the path a
    /// runner's failure report takes, and logs it under its code.
    ///
    /// # Errors
    /// The settlement's own refusal or datastore failure; the run then stays
    /// active and lapses as it would have, which is never worse than before.
    pub(super) async fn fail_unframed(&self, unframed: &Unframed) -> Result<()> {
        let detail = detail();
        let request = unframed.report(&detail);
        let received = self.gateway.inner.dependencies.wall_clock.now();
        if unframed.draft {
            self.runs
                .report_draft(&self.creations, &self.handle.runner, &request, received)
                .await?;
        } else {
            let mut clock = VerbClock::<ReportStage>::start();
            let _settled = self
                .runs
                .report_with(&self.handle.runner, &request, received, None, &mut clock)
                .await?;
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = error_code::RUN_FRAME_TOO_LARGE.as_str();
        let (run_id, turn_id) = (unframed.run_id.as_str(), unframed.turn_id.as_str());
        let runner_id = self.handle.runner.as_str();
        {
            let event = &EVENT_UNFRAMED;
            tracing::warn!(event, error_code, run_id, turn_id, runner_id);
        };
        Ok(())
    }
}

#[cfg(test)]
#[path = "unframed_tests.rs"]
mod tests;
