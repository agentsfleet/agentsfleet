//! The six cron tools, onto the schedules verb.
//!
//! Each parses its arguments, proves any schedule id is one before it can
//! reach a path, and hands `agentsfleetd`'s answer back to the model as it
//! came: the schedule's view, the list, the run, or the refusal with its code.
//! A message a schedule will hand a later run is masked for every token the
//! lease minted before it leaves, as a line to the thread is; `agentsfleetd`
//! masks the fleet's stored secrets.

use afd_core::id::Uuid7;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{ScheduleCall, answered};
use crate::catalog::{CRON_ADD, CRON_LIST, CRON_REMOVE, CRON_RUN, CRON_RUNS, CRON_UPDATE, Entry};
use crate::egress;
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// What a schedule id that is not one reads back.
const DETAIL_SCHEDULE_ID: &str = "schedule_id must be a schedule's id, as cron_list names it";

/// `cron_add`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Add {
    /// Five numeric fields: minute, hour, day of month, month, day of week,
    /// such as `0 9 * * 1` for Mondays at 09:00.
    cron: String,
    /// An IANA zone such as `Asia/Kolkata`; UTC when absent.
    timezone: Option<String>,
    /// What this fleet is asked to do each time it fires.
    message: String,
}

/// `cron_list`'s arguments: none.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "schemars renders a unit struct as `null`; the braces make it the empty object every provider's function wire expects"
)]
pub(crate) struct List {}

/// The arguments naming one schedule: `cron_remove`'s and `cron_run`'s.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Named {
    /// The schedule's id, as `cron_list` names it.
    schedule_id: String,
}

/// `cron_update`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    /// The schedule's id, as `cron_list` names it. Only a schedule this fleet
    /// created can be changed.
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

/// `cron_runs`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Runs {
    /// The schedule's id, as `cron_list` names it.
    schedule_id: String,
    /// How many runs to read, 1 to 100; 50 when absent.
    limit: Option<u32>,
    /// The `next_cursor` of the page before, to read further back.
    starting_after: Option<String>,
}

/// Adds a recurring schedule for this fleet.
#[derive(Debug)]
pub(crate) struct CronAdd;

#[async_trait::async_trait]
impl Handler for CronAdd {
    const ENTRY: &'static Entry = &CRON_ADD;
    const DESCRIPTION: &'static str = "Schedule this fleet to run again on a recurring cron \
        expression, with a message saying what to do each time.";
    type Arguments = Add;

    async fn run(&self, arguments: Add, context: ToolContext<'_, '_>) -> ToolOutput {
        let message = egress::masked(context.lease, arguments.message);
        let call = ScheduleCall::Create {
            cron: &arguments.cron,
            timezone: arguments.timezone.as_deref(),
            message: &message,
            once: false,
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// Lists this fleet's schedules.
#[derive(Debug)]
pub(crate) struct CronList;

#[async_trait::async_trait]
impl Handler for CronList {
    const ENTRY: &'static Entry = &CRON_LIST;
    const DESCRIPTION: &'static str = "List this fleet's schedules, each naming its source: \
        `fleet` for one this fleet made, `api` or `trigger` for one a person made.";
    type Arguments = List;

    async fn run(&self, _arguments: List, context: ToolContext<'_, '_>) -> ToolOutput {
        answered(context.lease.verbs.schedules(ScheduleCall::List).await)
    }
}

/// Removes a schedule this fleet made.
#[derive(Debug)]
pub(crate) struct CronRemove;

#[async_trait::async_trait]
impl Handler for CronRemove {
    const ENTRY: &'static Entry = &CRON_REMOVE;
    const DESCRIPTION: &'static str = "Delete a schedule this fleet created.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: ToolContext<'_, '_>) -> ToolOutput {
        let schedule = match schedule_id(&arguments.schedule_id) {
            Ok(schedule) => schedule,
            Err(refused) => return refused,
        };
        let call = ScheduleCall::Delete {
            schedule: &schedule,
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// Changes a schedule this fleet made.
#[derive(Debug)]
pub(crate) struct CronUpdate;

#[async_trait::async_trait]
impl Handler for CronUpdate {
    const ENTRY: &'static Entry = &CRON_UPDATE;
    const DESCRIPTION: &'static str = "Change the expression, zone or message of a schedule \
        this fleet created, or pause and resume it.";
    type Arguments = Update;

    async fn run(&self, arguments: Update, context: ToolContext<'_, '_>) -> ToolOutput {
        let schedule = match schedule_id(&arguments.schedule_id) {
            Ok(schedule) => schedule,
            Err(refused) => return refused,
        };
        let message = arguments
            .message
            .map(|text| egress::masked(context.lease, text));
        let call = ScheduleCall::Update {
            schedule: &schedule,
            cron: arguments.cron.as_deref(),
            timezone: arguments.timezone.as_deref(),
            message: message.as_deref(),
            paused: arguments.paused,
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// Runs a schedule now.
#[derive(Debug)]
pub(crate) struct CronRun;

#[async_trait::async_trait]
impl Handler for CronRun {
    const ENTRY: &'static Entry = &CRON_RUN;
    const DESCRIPTION: &'static str = "Run one of this fleet's schedules now, as if it had \
        fired; the run is queued behind this one.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: ToolContext<'_, '_>) -> ToolOutput {
        let schedule = match schedule_id(&arguments.schedule_id) {
            Ok(schedule) => schedule,
            Err(refused) => return refused,
        };
        let call = ScheduleCall::Run {
            schedule: &schedule,
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// Lists a schedule's runs.
#[derive(Debug)]
pub(crate) struct CronRuns;

#[async_trait::async_trait]
impl Handler for CronRuns {
    const ENTRY: &'static Entry = &CRON_RUNS;
    const DESCRIPTION: &'static str = "List the runs one of this fleet's schedules fired, \
        newest first.";
    type Arguments = Runs;

    async fn run(&self, arguments: Runs, context: ToolContext<'_, '_>) -> ToolOutput {
        let schedule = match schedule_id(&arguments.schedule_id) {
            Ok(schedule) => schedule,
            Err(refused) => return refused,
        };
        let call = ScheduleCall::Runs {
            schedule: &schedule,
            limit: arguments.limit,
            starting_after: arguments.starting_after.as_deref(),
        };
        answered(context.lease.verbs.schedules(call).await)
    }
}

/// The schedule `raw` names, proved an id before it can reach a path.
fn schedule_id(raw: &str) -> Result<Uuid7, ToolOutput> {
    Uuid7::parse(raw)
        .map_err(|_shape| ToolOutput::failed(ToolErrorCode::InvalidArguments, DETAIL_SCHEDULE_ID))
}

#[cfg(test)]
#[path = "schedules/tests.rs"]
mod tests;
