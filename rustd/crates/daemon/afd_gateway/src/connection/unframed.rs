//! A run whose assignment committed but whose frame never left the daemon.
//!
//! The run row is written before its frame is built, so a frame that is never
//! sent leaves a run no runner holds. Left alone it would lapse, be reassigned
//! at the next fence and, for a frame too large, fail the same way until
//! `EVENT_RUNS_MAX` ended it, the fleet's queue stalled behind it all the
//! while. What it is owed turns on whether another try could send it:
//!
//! - [`Cause::TooLarge`]: the assignment, measured before it is framed, passes
//!   [`MESSAGE_BYTES_MAX`]. No retry fits it, so it is settled at once, the way
//!   a runner reports a run it could not finish: under its own fence, its
//!   turn failed with [`FailureClass::FrameTooLarge`] and
//!   [`error_code::RUN_FRAME_TOO_LARGE`], nothing sent and nothing billed.
//!   What fills a frame is the message (`event.request_json`) and the
//!   instructions, each up to the text ceiling; history already dropped its
//!   oldest turns to fit (`afd_fleet::run::history::fit_frame`).
//! - [`Cause::Undelivered`]: the frame never reached the connection's writer,
//!   because its memory was not free before the deadline or the connection
//!   closed first. Another pass can send it, so the run is released
//!   (`afd_fleet`'s `Runs::release_unsent`) and its turn goes back to the
//!   queue at once. A frame the writer accepted may have reached the runner,
//!   so it is never released here: its fence decides.

use std::borrow::Cow;

use af_core::error_code;
use af_core::id::Uuid7;
use af_core::problem::Problem;
use af_observability::verb_span::{ReportStage, VerbClock};
use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::report::{FailureClass, Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::{SetupReport, TaggedRun};

use super::{ConnectionServices, Message};
use crate::Result;

/// The event a run failed for its frame is logged under.
const EVENT_UNFRAMED: &str = "run_frame_too_large";
/// The event a run released for an undelivered frame is logged under.
const EVENT_RELEASED: &str = "run_frame_undelivered_released";

/// Why a committed assignment was not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cause {
    /// Its frame would pass [`MESSAGE_BYTES_MAX`].
    TooLarge,
    /// Its frame never reached the connection's writer.
    Undelivered,
}

/// What a committed run that was not sent is owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Remedy {
    /// Failed for good: no retry could send it.
    Settle,
    /// Back on its queue at once: another pass can send it.
    Release,
}

impl Cause {
    /// A permanent fault settles its run; a transient one releases it.
    pub(super) const fn remedy(self) -> Remedy {
        match self {
            Self::TooLarge => Remedy::Settle,
            Self::Undelivered => Remedy::Release,
        }
    }
}

/// A committed run that was not sent: what failing it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unsent {
    /// The run the assignment wrote.
    run_id: String,
    /// The turn it was assigned.
    turn_id: String,
    /// The fence the assignment raised, which the failure report echoes.
    fencing_token: u64,
    /// Whether a setup draft owns the run, whose report carries its setup part.
    draft: bool,
}

impl Unsent {
    /// What failing `run` would need, kept until its frame is handed over.
    pub(super) fn of(run: &TaggedRun<'_>) -> Self {
        let TaggedRun::Fleet(payload) = run;
        Self {
            run_id: payload.run_id.clone().into_owned(),
            turn_id: payload.event.turn_id.clone().into_owned(),
            fencing_token: payload.fencing_token,
            draft: payload.owner.setup().is_some(),
        }
    }

    /// Whether `message` fits one frame, measured without allocating it.
    pub(super) fn fits(message: &Message<'_>) -> bool {
        af_validate::serialized_within(message, MESSAGE_BYTES_MAX)
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
    /// Gives `unsent`'s run what [`Cause::remedy`] says it is owed.
    ///
    /// # Errors
    /// The settlement's or release's own refusal or datastore failure; the
    /// run then stays active and lapses as it would have.
    pub(super) async fn not_sent(&self, unsent: &Unsent, cause: Cause) -> Result<()> {
        match cause.remedy() {
            Remedy::Settle => self.fail_unframed(unsent).await,
            Remedy::Release => self.release_unsent(unsent).await,
        }
    }

    /// Fails `unsent`'s run and its turn at once, through the path a
    /// runner's failure report takes, and logs it under its code.
    async fn fail_unframed(&self, unsent: &Unsent) -> Result<()> {
        let detail = detail();
        let request = unsent.report(&detail);
        let received = self.gateway.inner.dependencies.wall_clock.now();
        if unsent.draft {
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
        let (run_id, turn_id) = (unsent.run_id.as_str(), unsent.turn_id.as_str());
        let runner_id = self.handle.runner.as_str();
        {
            let event = &EVENT_UNFRAMED;
            tracing::warn!(event, error_code, run_id, turn_id, runner_id);
        };
        Ok(())
    }

    /// Puts `unsent`'s run back on its queue, fenced to this runner and the
    /// assignment's token, and logs whether it was still there to release.
    async fn release_unsent(&self, unsent: &Unsent) -> Result<()> {
        let run = Uuid7::parse(&unsent.run_id)?;
        let released = self
            .runs
            .runs
            .release_unsent(&run, &self.handle.runner, unsent.fencing_token)
            .await?;
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let (run_id, turn_id) = (unsent.run_id.as_str(), unsent.turn_id.as_str());
        let runner_id = self.handle.runner.as_str();
        let fencing_token = unsent.fencing_token;
        {
            let event = &EVENT_RELEASED;
            tracing::warn!(event, released, fencing_token, run_id, turn_id, runner_id);
        };
        Ok(())
    }
}

#[cfg(test)]
#[path = "unframed_tests.rs"]
mod tests;
