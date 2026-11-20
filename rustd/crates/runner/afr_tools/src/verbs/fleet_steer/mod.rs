//! `fleet_steer`: work this fleet puts on another fleet in its workspace.
//!
//! The text is masked for every token the run minted before it leaves, and
//! `agentsfleetd` masks the fleet's stored secrets. The tool answers at once:
//! the other fleet runs on its own, under its own setup and budget, and can
//! answer with this same tool. Its progress is read with `fleet_activity`.

use af_core::id::Uuid7;
use af_wire::steer_verb::FleetSteerAccepted;
use schemars::JsonSchema;
use serde::Deserialize;

use super::answered_with;
use crate::catalog::{Entry, FLEET_STEER};
use crate::egress;
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode};
use guffbolt::ToolOutput;

/// What a fleet id that is not one reads back.
pub(super) const DETAIL_FLEET_ID: &str = "fleet_id must be a fleet's id, as fleet_activity lists it or a message from that fleet names it";

/// `fleet_steer`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Steer {
    /// The fleet to ask: its `fleet_id` from `fleet_activity`, or from the
    /// first line of a message another fleet sent this one.
    fleet_id: String,
    /// What to ask it, in plain words. At most 1,048,576 characters.
    text: String,
}

/// Asks another fleet in this workspace to do something.
#[derive(Debug)]
pub(crate) struct FleetSteer;

#[async_trait::async_trait]
impl Handler for FleetSteer {
    const ENTRY: &'static Entry = &FLEET_STEER;
    const DESCRIPTION: &'static str = "Ask another fleet in this workspace to do something, \
        by its fleet_id. Returns at once: that fleet runs on its own and can answer with \
        fleet_steer. Read what it did with fleet_activity.";
    type Arguments = Steer;

    async fn run(&self, arguments: Steer, context: ToolContext<'_, '_>) -> ToolOutput {
        let Ok(fleet) = Uuid7::parse(&arguments.fleet_id) else {
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, DETAIL_FLEET_ID);
        };
        let text = egress::masked(context.run, arguments.text).await;
        answered_with(
            context.run.verbs.fleet_steer(&fleet, &text).await,
            |accepted| ToolOutput::succeeded(sentence(&fleet, &accepted)),
        )
    }
}

/// What the model reads once the other fleet has the work.
fn sentence(fleet: &Uuid7, accepted: &FleetSteerAccepted<'_>) -> String {
    let turn_id = &accepted.turn_id;
    if accepted.replayed {
        format!(
            "fleet {fleet} already has this request as event {turn_id}; this is the same run, not a new one"
        )
    } else {
        format!(
            "asked fleet {fleet}: event {turn_id}. It runs on its own and can answer with fleet_steer."
        )
    }
}

#[cfg(test)]
mod tests;
