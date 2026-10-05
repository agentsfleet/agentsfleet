//! The `agentsfleetd` verbs a run's tools reach through: the fleet's own
//! schedules, and a line said to the event's thread.
//!
//! ```text
//!   cron_* / schedule ──┐
//!                       ├──► LeaseVerbs ──► agentsfleetd, fenced by the lease
//!   message ────────────┘        ▲
//!                                └── the supervisor's control plane
//! ```
//!
//! # The runner keeps no clock and no channel credential
//!
//! A schedule lives in `agentsfleetd` and fires through `QStash`; a line is
//! posted by `agentsfleetd`, which holds the Slack token. The tools here only
//! say what to do. [`LeaseVerbs`] is the seam the supervisor implements over
//! its control plane, carrying the lease's id and fencing token itself, so a
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

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_core::problem::Problem;

use crate::runtime::{ToolErrorCode, ToolOutput};

mod message;
mod once;
mod schedules;

pub(crate) use self::message::Message;
pub(crate) use self::once::ScheduleOnce;
pub(crate) use self::schedules::{CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate};

/// What the model reads when `agentsfleetd` could not be reached.
const DETAIL_UNREACHABLE: &str = "agentsfleetd did not answer; try again shortly";

/// What the model reads for a refusal that named no registry code.
const DETAIL_REFUSED: &str = "agentsfleetd refused the request";

/// What the model reads for a success that answered no body, such as a
/// schedule removed outright.
const DONE: &str = "done";

/// One schedules call, as a tool asks it: the fleet is the lease's, so none
/// names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleCall<'a> {
    /// Create a schedule for the fleet.
    Create {
        /// A five-field cron expression.
        cron: &'a str,
        /// The zone it is read in; UTC when absent.
        timezone: Option<&'a str>,
        /// What the fleet is asked to do when it fires.
        message: &'a str,
        /// Whether it retires after its first fire.
        once: bool,
    },
    /// List every schedule of the fleet.
    List,
    /// Change the named fields of a schedule the fleet made.
    Update {
        /// The schedule.
        schedule: &'a Uuid7,
        /// A new expression.
        cron: Option<&'a str>,
        /// A new zone.
        timezone: Option<&'a str>,
        /// A new message.
        message: Option<&'a str>,
        /// Whether it should stop firing.
        paused: Option<bool>,
    },
    /// Delete a schedule the fleet made.
    Delete {
        /// The schedule.
        schedule: &'a Uuid7,
    },
    /// Fire a schedule now.
    Run {
        /// The schedule.
        schedule: &'a Uuid7,
    },
    /// Read a schedule's runs, newest first.
    Runs {
        /// The schedule.
        schedule: &'a Uuid7,
        /// How many to read.
        limit: Option<u32>,
        /// The `next_cursor` of the page before.
        starting_after: Option<&'a str>,
    },
}

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
            Self::Refused(Some(code)) if code == error_code::MESSAGE_LIMIT_REACHED => {
                ToolErrorCode::MessageLimitReached
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

/// The `agentsfleetd` verbs one lease's tools send through.
///
/// Implemented by the supervisor over its control plane, which adds the
/// lease's id and fencing token to every call.
#[async_trait::async_trait]
pub trait LeaseVerbs: Send + Sync + fmt::Debug {
    /// Sends one schedules call and answers the reply body of a success.
    async fn schedules(&self, call: ScheduleCall<'_>) -> Result<String, Unanswered>;

    /// Posts one line to the event's thread and answers whether it landed.
    async fn message(&self, text: &str) -> Result<bool, Unanswered>;
}

/// Verbs with nothing behind them: every call is unanswered.
///
/// For a lease that runs with no control plane, in a suite, so a tool that
/// reaches for one says so rather than pretending it worked. Test-only, as
/// `afr_egress::testing::closed` is: production always has a plane.
#[cfg(any(test, feature = "test-util"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct Closed;

/// The one [`Closed`] every caller borrows.
#[cfg(any(test, feature = "test-util"))]
pub static CLOSED: Closed = Closed;

#[cfg(any(test, feature = "test-util"))]
#[async_trait::async_trait]
impl LeaseVerbs for Closed {
    async fn schedules(&self, _call: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn message(&self, _text: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
}

#[cfg(test)]
#[path = "verbs/tests.rs"]
mod tests;
