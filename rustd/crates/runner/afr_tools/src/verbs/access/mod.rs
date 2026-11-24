//! `request_access`: the fleet asks a person to approve one secret or
//! integration of its workspace, by name.
//!
//! ```text
//!  request_access {name, scope?} ─► POST L/access
//!    201, a card for this turn ─► the run ends waiting; Approve or Deny resumes it
//!    200, already approved     ─► the run goes on holding it
//!    200, an earlier card      ─► the run goes on without it
//!    refused                   ─► "[kind] sentence"; the run goes on
//! ```
//!
//! The run ends the way `ask_user` ends it: the call asks, the step's other
//! calls run, and the person's answer on the card becomes this call's result.
//! `agentsfleetd` decides every refusal — the vault, the connected
//! integrations, a GitHub scope, a secret's host — and raises the card
//! itself, so the report raises no question for it. The bounds the wire
//! declares are proved here first, so a request no daemon would read never
//! leaves the runner.

use std::borrow::Cow;

use af_wire::access_verb::AccessRequest;
use garde::Validate as _;
use guffbolt::ToolOutput;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{AccessAnswer, answered_with};
use crate::catalog::{Entry, REQUEST_ACCESS};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode};

/// `request_access`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    /// The secret's key in the workspace vault, such as `sentry`, or a
    /// connected integration, such as `github`.
    name: String,
    /// For `github` only: `{"repositories": ["owner/name"], "access": "read"}`,
    /// or `"access": "write"` with `"base"`, the branch every pull request
    /// opens against.
    scope: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Asks a person to approve one secret or integration for this fleet.
#[derive(Debug)]
pub(crate) struct RequestAccess;

#[async_trait::async_trait]
impl Handler for RequestAccess {
    const ENTRY: &'static Entry = &REQUEST_ACCESS;
    const DESCRIPTION: &'static str = "Call this tool only when the person's newest message \
        asks for work that needs a secret or an integration you do not hold. Do not call it for \
        a question, a greeting or an approval; answer those in plain text. Ask for one name in \
        the workspace, such as a vault secret or `github` with its scope. A person approves it \
        once on a card: this run ends waiting, and resumes with their answer.";
    type Arguments = Request;

    async fn run(&self, arguments: Request, context: ToolContext<'_, '_>) -> ToolOutput {
        // The run's fence is the supervisor's to add: a tool never holds it.
        let request = AccessRequest {
            fencing_token: 0,
            name: Cow::Borrowed(&arguments.name),
            scope: arguments.scope,
        };
        if let Err(report) = request.validate() {
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, &report.to_string());
        }
        let name = arguments.name.as_str();
        answered_with(
            context.run.verbs.access(request).await,
            |answer| match answer {
                AccessAnswer::Asked => {
                    ToolOutput::asks(format!("Waiting for a person to approve access to {name}."))
                }
                AccessAnswer::Covered => ToolOutput::succeeded(format!(
                    "{name} is already approved for this fleet, and this run holds it."
                )),
                AccessAnswer::Pending => ToolOutput::succeeded(format!(
                    "An earlier request for {name} still waits on a person's approval; this run \
                     does not hold it."
                )),
            },
        )
    }
}

#[cfg(test)]
mod tests;
