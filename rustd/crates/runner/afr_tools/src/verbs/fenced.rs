//! `agentsfleetd` as the run's scheduler and channel: `guffbolt`'s cron
//! tools, `schedule` and `message`, built over the run's verbs.
//!
//! ```text
//!  cron_* / schedule ─► guffbolt::Scheduler ─┐
//!  message ───────────► guffbolt::Channel ───┴─► Fenced ─► RunVerbs ─► agentsfleetd
//! ```
//!
//! Each call proves any schedule id is one before it can reach a path, masks
//! a message for every token the run minted before it leaves, and hands
//! `agentsfleetd`'s answer back as it came: the schedule's view, the list, the
//! run, or the refusal with its code. `agentsfleetd` masks the fleet's stored
//! secrets, holds the channel's credential, and fences every call by the run.

use af_core::id::Uuid7;
use guffbolt::CallRef;
use guffbolt::tools::Refusal;
use guffbolt::tools::message::{Channel, ChannelFuture, Posted};
use guffbolt::tools::schedule::{Change, Job, Page, ScheduleFuture, Scheduler};

use super::{ScheduleCall, Unanswered};
use crate::egress;
use crate::run::Run;
use crate::runtime::ToolErrorCode;

/// What a schedule id that is not one reads back.
const DETAIL_SCHEDULE_ID: &str = "schedule_id must be a schedule's id, as cron_list names it";

/// The run's verbs, as its scheduler and its channel.
#[derive(Debug, Clone, Copy)]
pub struct Fenced<'b, 'run> {
    run: &'b Run<'run>,
}

impl<'b, 'run> Fenced<'b, 'run> {
    /// The verbs of `run`, masked by its guard.
    #[must_use]
    pub const fn new(run: &'b Run<'run>) -> Self {
        Self { run }
    }

    /// `text`, masked for every token the run minted.
    async fn masked(self, text: &str) -> String {
        egress::masked(self.run, text.to_owned()).await
    }

    /// Sends `call` and answers the reply body, or the refusal with its code.
    async fn schedules(self, call: ScheduleCall<'_>) -> Result<String, Refusal> {
        self.run.verbs.schedules(call).await.map_err(refusal)
    }
}

/// What `agentsfleetd` did not do, as the model reads it.
fn refusal(unanswered: Unanswered) -> Refusal {
    Refusal::new(unanswered.tool_code().into(), unanswered.detail())
}

/// The schedule `raw` names, proved an id before it can reach a path.
fn schedule_id(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw)
        .map_err(|_shape| Refusal::new(ToolErrorCode::InvalidArguments.into(), DETAIL_SCHEDULE_ID))
}

impl Scheduler for Fenced<'_, '_> {
    fn add<'a>(&'a self, _call: &'a CallRef, job: Job<'a>) -> ScheduleFuture<'a> {
        Box::pin(async move {
            let message = self.masked(job.message).await;
            let call = ScheduleCall::Create {
                cron: job.cron,
                timezone: job.timezone,
                message: &message,
                once: job.once,
            };
            self.schedules(call).await
        })
    }

    fn list(&self) -> ScheduleFuture<'_> {
        Box::pin(self.schedules(ScheduleCall::List))
    }

    fn update<'a>(
        &'a self,
        _call: &'a CallRef,
        id: &'a str,
        change: Change<'a>,
    ) -> ScheduleFuture<'a> {
        Box::pin(async move {
            let schedule = schedule_id(id)?;
            let message = match change.message {
                Some(text) => Some(self.masked(text).await),
                None => None,
            };
            let call = ScheduleCall::Update {
                schedule: &schedule,
                cron: change.cron,
                timezone: change.timezone,
                message: message.as_deref(),
                paused: change.paused,
            };
            self.schedules(call).await
        })
    }

    fn remove<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        Box::pin(async move {
            let schedule = schedule_id(id)?;
            self.schedules(ScheduleCall::Delete {
                schedule: &schedule,
            })
            .await
        })
    }

    fn run_now<'a>(&'a self, _call: &'a CallRef, id: &'a str) -> ScheduleFuture<'a> {
        Box::pin(async move {
            let schedule = schedule_id(id)?;
            self.schedules(ScheduleCall::Run {
                schedule: &schedule,
            })
            .await
        })
    }

    fn runs<'a>(&'a self, id: &'a str, page: Page<'a>) -> ScheduleFuture<'a> {
        Box::pin(async move {
            let schedule = schedule_id(id)?;
            let call = ScheduleCall::Runs {
                schedule: &schedule,
                limit: page.limit,
                starting_after: page.starting_after,
            };
            self.schedules(call).await
        })
    }
}

impl Channel for Fenced<'_, '_> {
    fn post<'a>(&'a self, _call: &'a CallRef, text: &'a str) -> ChannelFuture<'a> {
        Box::pin(async move {
            let text = self.masked(text).await;
            match self.run.verbs.message(&text).await {
                Ok(true) => Ok(Posted::Delivered),
                Ok(false) => Ok(Posted::NotTaken),
                Err(unanswered) => Err(refusal(unanswered)),
            }
        })
    }
}

#[cfg(test)]
#[path = "fenced_tests.rs"]
mod tests;
