//! The `agentsfleetd` verbs a run's tools reach through.
//!
//! The fleet's own schedules and triggers, a line said to the event's thread,
//! the workspace's other fleets, a request for access, and the durable tasks
//! a person's message starts or steers.
//!
//! ```text
//!   cron_* / schedule ─► Scheduler ─┐
//!   message ───────────► Channel ───┴─► Fenced ─┐
//!   fleet_steer, fleet_activity ────────────────┤
//!   trigger_add, trigger_list, trigger_remove ──┼──► RunVerbs ──► agentsfleetd,
//!   request_access ─────────────────────────────┤        ▲         fenced by the run
//!   task_start, task_input ─────────────────────┘        └── the supervisor's control plane
//! ```
//!
//! The cron tools, `schedule` and `message` are `guffbolt`'s, built over
//! [`Fenced`], the run's verbs as their scheduler and channel; the rest are
//! this crate's own.
//!
//! # The runner keeps no clock and no channel credential
//!
//! A schedule lives in `agentsfleetd` and fires through `QStash`; a line is
//! posted by `agentsfleetd`, which holds the Slack token. The tools here only
//! say what to do. [`RunVerbs`] is the seam the supervisor implements over
//! its control plane, carrying the run's id and fencing token itself, so a
//! tool never holds the token and a fake answers every call in a suite.
//!
//! # A refusal reaches the model with its code
//!
//! [`Unanswered`] keeps the registry code `agentsfleetd` refused with, and
//! [`answered`] turns it into the tool error the model reads: the codes a
//! model acts on differently — a full schedule cap, a person's schedule, a
//! schedule that will not run now, no thread, a spent message budget — each
//! have their own, and the rest read as a refusal naming its code.

use std::borrow::Cow;
use std::fmt;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::problem::Problem;
use af_wire::access_verb::AccessRequest;
use af_wire::steer_verb::FleetSteerAccepted;
use guffbolt::ToolOutput;

use crate::runtime::ToolErrorCode;

mod access;
mod calls;
mod durable_task;
mod fenced;
mod fleet_steer;
mod fleets;
mod tasks;
mod triggers;

pub use durable_task::DecisionEvaluation;

pub(crate) use self::access::RequestAccess;
pub use self::calls::{AccessAnswer, FleetsCall, NewTrigger, ScheduleCall, TriggerCall};
pub use self::fenced::Fenced;
pub(crate) use self::fleet_steer::FleetSteer;
pub(crate) use self::fleets::FleetActivity;
pub(crate) use self::tasks::{TaskInput, TaskStart};
pub(crate) use self::triggers::{TriggerAdd, TriggerList, TriggerRemove};

/// What the model reads when `agentsfleetd` could not be reached.
const DETAIL_UNREACHABLE: &str = "agentsfleetd did not answer; try again shortly";

/// What the model reads for a refusal that named no registry code.
const DETAIL_REFUSED: &str = "agentsfleetd refused the request";

/// What the model reads for a success that answered no body, such as a
/// schedule removed outright.
const DONE: &str = "done";

/// Why `agentsfleetd` did not do what a tool asked.
///
/// A fieldless-in-spirit vocabulary kept a plain `Copy` enum, the way
/// [`ToolErrorCode`] is: a tool discriminates on it and it carries no cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unanswered {
    /// It answered no, naming the registry code its problem carried, if any.
    Refused(Option<ErrorCode>),
    /// It could not be reached, or stayed unavailable through every attempt.
    Unreachable,
}

impl Unanswered {
    /// The tool error the model reads.
    #[must_use]
    pub fn tool_code(self) -> ToolErrorCode {
        match self {
            Self::Unreachable => ToolErrorCode::AgentsfleetdUnreachable,
            Self::Refused(Some(code)) if code == error_code::SCHEDULE_CAP_REACHED => {
                ToolErrorCode::ScheduleCapReached
            }
            Self::Refused(Some(code)) if code == error_code::SCHEDULE_NOT_FLEET_OWNED => {
                ToolErrorCode::ScheduleNotFleetOwned
            }
            Self::Refused(Some(code)) if code == error_code::SCHEDULE_NOT_RUNNABLE => {
                ToolErrorCode::ScheduleNotRunnable
            }
            Self::Refused(Some(code)) if code == error_code::MESSAGE_NO_CHANNEL => {
                ToolErrorCode::MessageNoChannel
            }
            Self::Refused(_other) => ToolErrorCode::AgentsfleetdRefused,
        }
    }

    /// The sentence the model reads after the code: the registry code and
    /// what the problem table says to do about it.
    #[must_use]
    pub fn detail(self) -> Cow<'static, str> {
        match self {
            Self::Unreachable => Cow::Borrowed(DETAIL_UNREACHABLE),
            Self::Refused(None) => Cow::Borrowed(DETAIL_REFUSED),
            Self::Refused(Some(code)) => {
                Cow::Owned(format!("{}: {}", code.as_str(), Problem::of(code).hint()))
            }
        }
    }
}

/// The call's output: the reply body on success, the refusal otherwise.
pub(crate) fn answered(reply: Result<String, Unanswered>) -> ToolOutput {
    answered_with(reply, |body| {
        if body.is_empty() {
            ToolOutput::succeeded(DONE)
        } else {
            ToolOutput::succeeded(body)
        }
    })
}

/// The call's output: what `render` makes of a success, the refusal otherwise.
pub(crate) fn answered_with<T>(
    reply: Result<T, Unanswered>,
    render: impl FnOnce(T) -> ToolOutput,
) -> ToolOutput {
    match reply {
        Ok(answer) => render(answer),
        Err(unanswered) => ToolOutput::failed(unanswered.tool_code(), &unanswered.detail()),
    }
}

/// The `agentsfleetd` verbs one run's tools send through.
///
/// Implemented by the supervisor over its control plane, which adds the
/// run's id and fencing token to every call.
#[async_trait::async_trait]
pub trait RunVerbs: Send + Sync + fmt::Debug {
    /// Sends one schedules call and answers the reply body of a success.
    async fn schedules(&self, call: ScheduleCall<'_>) -> Result<String, Unanswered>;

    /// Posts one line to the event's thread for the run's call
    /// `call_number`, and answers whether it landed.
    async fn message(&self, call_number: u64, text: &str) -> Result<bool, Unanswered>;

    /// Reads the workspace's fleets, or one fleet's events, and answers the
    /// reply body as it came.
    async fn fleets(&self, _call: FleetsCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Sends one triggers call and answers the reply body of a success.
    async fn triggers(&self, _call: TriggerCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Asks a person to approve `request`'s name for the fleet, fenced by the
    /// run whatever fence `request` names, and answers what the request did.
    async fn access(&self, _request: AccessRequest<'_>) -> Result<AccessAnswer, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Puts `text` on another fleet in the workspace and answers the event it
    /// became.
    async fn fleet_steer(
        &self,
        _fleet: &Uuid7,
        _text: &str,
    ) -> Result<FleetSteerAccepted<'static>, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Read complete native input and accepted recovery meaning at a safe boundary.
    async fn durable_task_inputs(
        &self,
        _query: af_wire::durable_task::DurableTaskInputsQuery,
    ) -> Result<af_wire::durable_task::DurableTaskInputsResponse<'static>, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Binds one span of a person's message to a new or unfinished task,
    /// fenced by the run whatever fence `input` names.
    async fn durable_task_bindings(
        &self,
        _input: af_wire::durable_task::DurableTaskBindingsInput,
    ) -> Result<af_wire::durable_task::DurableTaskBindingsResponse<'static>, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Acknowledge an accepted input or answer without replaying its meaning.
    async fn durable_task_input_receipt(
        &self,
        _input: af_wire::durable_task::DurableTaskInputReceiptInput,
    ) -> Result<af_wire::durable_task::DurableTaskInputReceiptResult, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Reserve, evaluate and account one decision outside model/sandbox custody.
    async fn durable_task_decision(
        &self,
        _input: af_wire::durable_task::DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Publish a tested tree through the native exact-head effect owner.
    async fn durable_task_publish(
        &self,
        _input: af_wire::durable_task::DurableTaskEffectInput,
    ) -> Result<af_wire::durable_task::DurableTaskEffect, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Read back one original publication; this never authorizes a new write.
    async fn durable_task_effect(
        &self,
        _effect_id: &str,
    ) -> Result<af_wire::durable_task::DurableTaskEffect, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Recover all effects beyond a checkpoint's confirmed watermark.
    async fn durable_task_effects(
        &self,
        _query: af_wire::durable_task::DurableTaskEffectsQuery,
    ) -> Result<af_wire::durable_task::DurableTaskEffectsPage, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Reads the compacted context of the conversation the run continues,
    /// under the run's fence: its scope, the stored version and the saved
    /// checkpoint. A refusal leaves the run on its earlier turns alone.
    async fn context_load(&self) -> Result<af_wire::context::ContextLoadResponse, Unanswered> {
        Err(Unanswered::Refused(None))
    }

    /// Saves a compaction's checkpoint over the version `request` expects,
    /// under the run's fence, whatever fence `request` names; a superseded
    /// run's save is refused.
    async fn context_save(
        &self,
        _request: af_wire::context::ContextSaveRequest,
    ) -> Result<af_wire::context::ContextSaveResponse, Unanswered> {
        Err(Unanswered::Refused(None))
    }
}

/// Verbs with nothing behind them: every call is unanswered.
///
/// For a run that runs with no control plane, in a suite, so a tool that
/// reaches for one says so rather than pretending it worked. Test-only, as
/// `af_egress::testing::closed` is: production always has a plane.
#[cfg(any(test, feature = "test-util"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Closed;

/// The one [`Closed`] every caller borrows.
#[cfg(any(test, feature = "test-util"))]
pub static CLOSED: Closed = Closed;

#[cfg(any(test, feature = "test-util"))]
#[async_trait::async_trait]
impl RunVerbs for Closed {
    async fn schedules(&self, _call: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn message(&self, _call_number: u64, _text: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
}

#[cfg(test)]
mod tests;
