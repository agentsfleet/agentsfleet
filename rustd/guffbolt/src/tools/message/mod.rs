//! `message`: one line said to the conversation this run was asked from,
//! before the answer, over the [`Channel`] it is built with.
//!
//! The channel decides where the line goes: a callback of the embedder's
//! ([`LocalChannel`]), or a service that posts it to a thread and holds the
//! credential to do so, so the run never does.

mod local;

use std::fmt;
use std::pin::Pin;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, Refusal, code};
use crate::transcript::CallRef;

pub use self::local::LocalChannel;

/// What a line that landed reads back.
const DELIVERED: &str = "posted to the conversation";
/// What a line the channel did not take reads back.
const NOT_DELIVERED: &str =
    "the conversation did not take the message; say it in the answer instead";

/// One channel call's work: whether the line landed, or the refusal.
pub type ChannelFuture<'a> = Pin<Box<dyn Future<Output = Result<Posted, Refusal>> + Send + 'a>>;

/// What became of a line the channel was handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posted {
    /// The conversation took it.
    Delivered,
    /// The channel answered, and the conversation did not take it.
    NotTaken,
}

/// Where a run's lines to its conversation go.
pub trait Channel: Send + Sync + fmt::Debug {
    /// Posts `text`; the channel keys a retry of the same call by `call`.
    fn post<'a>(&'a self, call: &'a CallRef, text: &'a str) -> ChannelFuture<'a>;
}

/// `message`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Say {
    /// What to tell the people in the conversation now, such as what this
    /// run is doing or waiting for.
    text: String,
}

backed! {
    /// Posts one line to the conversation.
    Message over Channel
}

impl Handler for Message<'_> {
    const NAME: &'static str = "message";
    const DESCRIPTION: &'static str = "Post a short line to the conversation this run was \
        asked from, before the answer, such as progress or what you are waiting on.";
    type Arguments = Say;

    async fn run(&self, arguments: Say, context: Context<'_>) -> ToolOutput {
        match self.backend.post(context.call_ref, &arguments.text).await {
            Ok(Posted::Delivered) => ToolOutput::succeeded(DELIVERED),
            Ok(Posted::NotTaken) => ToolOutput::failed(code::UPSTREAM_UNREACHABLE, NOT_DELIVERED),
            Err(refusal) => refusal.into(),
        }
    }
}

#[cfg(test)]
mod tests;
