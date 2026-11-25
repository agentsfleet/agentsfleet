//! `trigger_add`, `trigger_list` and `trigger_remove`: what may wake the
//! fleet, as rows it adds itself.
//!
//! ```text
//!  trigger_add {kind, source?, events?, repositories?, verifier?, secret_name?, channel?}
//!     ─► POST L/triggers, naming the call's item ─► the trigger, or "[kind] sentence"
//!  trigger_list            ─► GET L/triggers      ─► every trigger of the fleet
//!  trigger_remove {id}     ─► DELETE L/triggers/{id}
//! ```
//!
//! `agentsfleetd` reads an add through the one trigger parser and refuses a
//! provider the workspace has not connected, a signing secret its vault does
//! not hold and a second trigger on one source; the model reads each refusal
//! and may correct the call in the same run. An add carries the number the
//! run's ledger gave the call, so a retried add lands once.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::trigger_verb::{TriggerKind, TriggerVerifier};
use guffbolt::ToolOutput;
use guffbolt::tools::NoArguments;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{NewTrigger, TriggerCall, answered};
use crate::catalog::{Entry, TRIGGER_ADD, TRIGGER_LIST, TRIGGER_REMOVE};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode};

/// What a call the run's ledger never numbered reads back: its effect has no
/// item to land once under.
const DETAIL_UNNUMBERED: &str = "the call has no number in its run, so its effect cannot be keyed";

/// What a trigger id that is not one reads back.
const DETAIL_TRIGGER_ID: &str = "trigger_id must be a trigger's id, as trigger_list names it";

/// What wakes the fleet, as the model names it.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Kind {
    /// A signed delivery to the fleet's own webhook URL.
    Webhook,
    /// An event the workspace's GitHub App delivers.
    AppEvent,
    /// A mention of the bot in one Slack channel.
    Mention,
}

impl From<Kind> for TriggerKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Webhook => Self::Webhook,
            Kind::AppEvent => Self::AppEvent,
            Kind::Mention => Self::Mention,
        }
    }
}

/// How a webhook from a source the workspace does not know signs its body.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Verifier {
    /// The signing recipe, such as `body-hex-sha256` or `svix-v1`.
    scheme: String,
    /// The header carrying the signature, for a recipe that signs the body
    /// alone.
    header: Option<String>,
}

/// `trigger_add`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Add {
    /// What wakes the fleet.
    kind: Kind,
    /// For `webhook`: the source, the last segment of its URL, such as
    /// `sentry`.
    source: Option<String>,
    /// The event names that wake the fleet; every event when absent.
    events: Option<Vec<String>>,
    /// For `app_event`: the repositories whose events wake the fleet, as
    /// `owner/name`.
    repositories: Option<Vec<String>>,
    /// For a `webhook` from a source the workspace does not know: how it
    /// signs.
    verifier: Option<Verifier>,
    /// For `webhook`: the workspace vault key holding its signing secret.
    secret_name: Option<String>,
    /// For `mention`: the Slack channel's id.
    channel: Option<String>,
}

/// Every text of `texts`, borrowed.
fn borrowed(texts: &[String]) -> Vec<Cow<'_, str>> {
    texts
        .iter()
        .map(|text| Cow::Borrowed(text.as_str()))
        .collect()
}

/// Adds what may wake the fleet.
#[derive(Debug)]
pub(crate) struct TriggerAdd;

#[async_trait::async_trait]
impl Handler for TriggerAdd {
    const ENTRY: &'static Entry = &TRIGGER_ADD;
    const DESCRIPTION: &'static str = "Call this tool only when the person's newest message \
        asks for this fleet to wake on something outside it. Do not call it for a question, a \
        greeting or an approval; answer those in plain text. Adds a webhook, a GitHub App event \
        or a Slack mention the workspace has connected; a webhook's URL is in the answer. A \
        webhook's signing secret stays in the workspace vault, named by secret_name: \
        agentsfleetd checks deliveries with it, so this fleet needs no access to it.";
    type Arguments = Add;

    async fn run(&self, arguments: Add, context: ToolContext<'_, '_>) -> ToolOutput {
        let Some(call_number) = context.run.calls.of(&context.call_ref.call_id) else {
            return ToolOutput::failed(ToolErrorCode::AgentsfleetdRefused, DETAIL_UNNUMBERED);
        };
        let trigger = NewTrigger {
            kind: arguments.kind.into(),
            source: arguments.source.as_deref().map(Cow::Borrowed),
            events: arguments.events.as_deref().map(borrowed),
            repositories: arguments.repositories.as_deref().map(borrowed),
            verifier: arguments.verifier.as_ref().map(|verifier| TriggerVerifier {
                scheme: Cow::Borrowed(&verifier.scheme),
                header: verifier.header.as_deref().map(Cow::Borrowed),
            }),
            secret_name: arguments.secret_name.as_deref().map(Cow::Borrowed),
            channel: arguments.channel.as_deref().map(Cow::Borrowed),
        };
        let call = TriggerCall::Add {
            call_number,
            trigger,
        };
        answered(context.run.verbs.triggers(call).await)
    }
}

/// Lists what may wake the fleet.
#[derive(Debug)]
pub(crate) struct TriggerList;

#[async_trait::async_trait]
impl Handler for TriggerList {
    const ENTRY: &'static Entry = &TRIGGER_LIST;
    const DESCRIPTION: &'static str =
        "List the triggers that wake this fleet, each with its id, kind, source and URL.";
    type Arguments = NoArguments;

    async fn run(&self, _arguments: NoArguments, context: ToolContext<'_, '_>) -> ToolOutput {
        answered(context.run.verbs.triggers(TriggerCall::List).await)
    }
}

/// `trigger_remove`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Remove {
    /// The trigger's id, as `trigger_list` names it.
    trigger_id: String,
}

/// Removes one trigger of the fleet.
#[derive(Debug)]
pub(crate) struct TriggerRemove;

#[async_trait::async_trait]
impl Handler for TriggerRemove {
    const ENTRY: &'static Entry = &TRIGGER_REMOVE;
    const DESCRIPTION: &'static str = "Remove one trigger of this fleet, by its id, only when \
        the person's newest message asks; deliveries to it are refused from then on.";
    type Arguments = Remove;

    async fn run(&self, arguments: Remove, context: ToolContext<'_, '_>) -> ToolOutput {
        let Ok(trigger) = Uuid7::parse(&arguments.trigger_id) else {
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, DETAIL_TRIGGER_ID);
        };
        let call = TriggerCall::Remove { trigger: &trigger };
        answered(context.run.verbs.triggers(call).await)
    }
}

#[cfg(test)]
mod tests;
