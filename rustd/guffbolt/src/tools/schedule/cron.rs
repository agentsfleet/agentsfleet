//! The six cron tools: each parses its arguments and asks its scheduler.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

use super::{Change, Job, Page, Scheduler, answered};
use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, NoArguments};

/// `cron_add`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Add {
    /// Five numeric fields: minute, hour, day of month, month, day of week,
    /// such as `0 9 * * 1` for Mondays at 09:00.
    cron: String,
    /// An IANA zone such as `Asia/Kolkata`; UTC when absent.
    timezone: Option<String>,
    /// What the run is asked to do each time it fires.
    message: String,
}

/// `cron_list`'s arguments: none.
#[derive(Debug, Clone, Copy, JsonSchema)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "schemars renders a unit struct as `null`; the braces make it the empty object every provider's function wire expects"
)]
pub struct List {}

impl<'de> Deserialize<'de> for List {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        NoArguments::deserialize(deserializer).map(|_none| Self {})
    }
}

/// The arguments naming one schedule: `cron_remove`'s and `cron_run`'s.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The schedule's id, as `cron_list` names it.
    schedule_id: String,
}

/// `cron_update`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Update {
    /// The schedule's id, as `cron_list` names it.
    schedule_id: String,
    /// A new five-field expression.
    cron: Option<String>,
    /// A new IANA zone.
    timezone: Option<String>,
    /// A new message.
    message: Option<String>,
    /// `true` stops it firing, `false` starts it again.
    paused: Option<bool>,
}

/// `cron_runs`' arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Runs {
    /// The schedule's id, as `cron_list` names it.
    schedule_id: String,
    /// How many runs to read, 1 to 100; 50 when absent.
    limit: Option<u32>,
    /// The `next_cursor` of the page before, to read further back.
    starting_after: Option<String>,
}

backed! {
    /// Adds a recurring schedule.
    CronAdd over Scheduler
}

impl Handler for CronAdd<'_> {
    const NAME: &'static str = "cron_add";
    const DESCRIPTION: &'static str = "Schedule a run on a recurring cron expression, with a \
        message saying what to do each time it fires.";
    type Arguments = Add;

    async fn run(&self, arguments: Add, context: Context<'_>) -> ToolOutput {
        let job = Job {
            cron: &arguments.cron,
            timezone: arguments.timezone.as_deref(),
            message: &arguments.message,
            once: false,
        };
        answered(self.backend.add(context.call_ref, job).await)
    }
}

backed! {
    /// Lists the schedules.
    CronList over Scheduler
}

impl Handler for CronList<'_> {
    const NAME: &'static str = "cron_list";
    const DESCRIPTION: &'static str =
        "List the schedules, each with its expression, zone, message and who made it.";
    type Arguments = List;

    async fn run(&self, _arguments: List, _context: Context<'_>) -> ToolOutput {
        answered(self.backend.list().await)
    }
}

backed! {
    /// Removes a schedule.
    CronRemove over Scheduler
}

impl Handler for CronRemove<'_> {
    const NAME: &'static str = "cron_remove";
    const DESCRIPTION: &'static str = "Delete a schedule.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: Context<'_>) -> ToolOutput {
        let removed = self
            .backend
            .remove(context.call_ref, &arguments.schedule_id)
            .await;
        answered(removed)
    }
}

backed! {
    /// Changes a schedule.
    CronUpdate over Scheduler
}

impl Handler for CronUpdate<'_> {
    const NAME: &'static str = "cron_update";
    const DESCRIPTION: &'static str = "Change a schedule's expression, zone or message, or \
        pause and resume it.";
    type Arguments = Update;

    async fn run(&self, arguments: Update, context: Context<'_>) -> ToolOutput {
        let change = Change {
            cron: arguments.cron.as_deref(),
            timezone: arguments.timezone.as_deref(),
            message: arguments.message.as_deref(),
            paused: arguments.paused,
        };
        let updated = self
            .backend
            .update(context.call_ref, &arguments.schedule_id, change)
            .await;
        answered(updated)
    }
}

backed! {
    /// Runs a schedule now.
    CronRun over Scheduler
}

impl Handler for CronRun<'_> {
    const NAME: &'static str = "cron_run";
    const DESCRIPTION: &'static str = "Run a schedule now, as if it had fired; the run is \
        queued behind this one.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: Context<'_>) -> ToolOutput {
        let fired = self
            .backend
            .run_now(context.call_ref, &arguments.schedule_id)
            .await;
        answered(fired)
    }
}

backed! {
    /// Lists a schedule's runs.
    CronRuns over Scheduler
}

impl Handler for CronRuns<'_> {
    const NAME: &'static str = "cron_runs";
    const DESCRIPTION: &'static str = "List the runs a schedule fired, newest first.";
    type Arguments = Runs;

    async fn run(&self, arguments: Runs, _context: Context<'_>) -> ToolOutput {
        let page = Page {
            limit: arguments.limit,
            starting_after: arguments.starting_after.as_deref(),
        };
        answered(self.backend.runs(&arguments.schedule_id, page).await)
    }
}
