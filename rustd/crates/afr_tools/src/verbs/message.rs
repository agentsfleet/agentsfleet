//! `message`: one line said to the thread this run was asked from, before
//! the answer.
//!
//! The text is masked for every token the lease minted before it leaves, and
//! `agentsfleetd` masks the fleet's stored secrets and posts it: the runner
//! never holds the channel's credential.

use schemars::JsonSchema;
use serde::Deserialize;

use super::answered_with;
use crate::catalog::{Entry, MESSAGE};
use crate::egress;
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// What a line that landed reads back.
const DELIVERED: &str = "posted to the thread";

/// What a line the channel refused, or never answered for, reads back.
const NOT_DELIVERED: &str = "the thread did not take the message; say it in the answer instead";

/// `message`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Say {
    /// What to tell the people in the thread now, such as what this run is
    /// doing or waiting for. At most 4096 bytes; at most 8 per run.
    text: String,
}

/// Posts one line to the event's thread.
#[derive(Debug)]
pub(crate) struct Message;

#[async_trait::async_trait]
impl Handler for Message {
    const ENTRY: &'static Entry = &MESSAGE;
    const DESCRIPTION: &'static str = "Post a short line to the thread this run was asked \
        from, before the answer, such as progress or what you are waiting on.";
    type Arguments = Say;

    async fn run(&self, arguments: Say, context: ToolContext<'_, '_>) -> ToolOutput {
        let text = egress::masked(context.lease, arguments.text).await;
        answered_with(context.lease.verbs.message(&text).await, |delivered| {
            if delivered {
                ToolOutput::succeeded(DELIVERED)
            } else {
                ToolOutput::failed(ToolErrorCode::UpstreamUnreachable, NOT_DELIVERED)
            }
        })
    }
}

#[cfg(test)]
#[path = "message/tests.rs"]
mod tests;
