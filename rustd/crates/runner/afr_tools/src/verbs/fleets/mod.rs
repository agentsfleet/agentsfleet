//! `fleet_activity`: which fleets this fleet's workspace holds, and what one
//! of them did recently.
//!
//! The answer is `agentsfleetd`'s as it came: the list, or one fleet's events
//! with an excerpt of what each was asked and answered. Reading a fleet wakes
//! nothing and costs that fleet no model call.

use af_core::id::Uuid7;
use schemars::JsonSchema;
use serde::Deserialize;

use super::fleet_steer::DETAIL_FLEET_ID;
use super::{FleetsCall, answered};
use crate::catalog::{Entry, FLEET_ACTIVITY};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode};
use guffbolt::ToolOutput;

/// `fleet_activity`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Activity {
    /// A fleet's id, to read its recent events. Omit it to list the
    /// workspace's fleets with their status.
    fleet_id: Option<String>,
    /// How many to read: up to 100 fleets, or up to 25 events.
    limit: Option<u32>,
    /// The `next_cursor` of the page before, to read the next one.
    starting_after: Option<String>,
}

/// Reads the workspace's fleets, or one fleet's recent events.
#[derive(Debug)]
pub(crate) struct FleetActivity;

#[async_trait::async_trait]
impl Handler for FleetActivity {
    const ENTRY: &'static Entry = &FLEET_ACTIVITY;
    const DESCRIPTION: &'static str = "List this workspace's fleets with their status, or, \
        given a fleet_id, that fleet's recent events with an excerpt of what it was asked \
        and what it answered. Wakes no fleet.";
    type Arguments = Activity;

    async fn run(&self, arguments: Activity, context: ToolContext<'_, '_>) -> ToolOutput {
        let starting_after = arguments.starting_after.as_deref();
        let Some(raw) = arguments.fleet_id.as_deref() else {
            let call = FleetsCall::List {
                limit: arguments.limit,
                starting_after,
            };
            return answered(context.run.verbs.fleets(call).await);
        };
        let Ok(fleet) = Uuid7::parse(raw) else {
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, DETAIL_FLEET_ID);
        };
        let call = FleetsCall::Events {
            fleet: &fleet,
            limit: arguments.limit,
            starting_after,
        };
        answered(context.run.verbs.fleets(call).await)
    }
}

#[cfg(test)]
mod tests;
