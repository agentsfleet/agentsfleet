//! `notify`: a notification to the person the run works for, over the
//! [`Notifier`] it is built with.
//!
//! guffbolt ships no local notifier, because every notifier is a vendor's:
//! the embedder builds `notify` over one of its own, which holds the vendor's
//! credential and sends through its network policy, so the model never names
//! a credential and an argument naming one is refused by the schema.

use std::fmt;
use std::pin::Pin;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, Refusal};
use crate::transcript::CallRef;

/// One notifier call's work: what the model reads back, or the refusal.
pub type NotifyFuture<'a> = Pin<Box<dyn Future<Output = Result<String, Refusal>> + Send + 'a>>;

/// How urgently a notification asks for attention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Delivered without a sound or an alert.
    Low,
    /// Delivered as the recipient's notifications usually are.
    #[default]
    Normal,
    /// Delivered past the recipient's quiet hours.
    High,
}

/// One notification, as the model wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notification<'a> {
    /// Its text.
    pub message: &'a str,
    /// A title above it.
    pub title: Option<&'a str>,
    /// How urgently it asks for attention.
    pub priority: Priority,
}

/// Where a run's notifications go.
pub trait Notifier: Send + Sync + fmt::Debug {
    /// Sends `note`, answering what the model reads of the vendor's reply;
    /// the notifier keys a retry of the same call by `call`.
    fn notify<'a>(&'a self, call: &'a CallRef, note: Notification<'a>) -> NotifyFuture<'a>;
}

/// `notify`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    /// The notification's text.
    message: String,
    /// A title above it.
    title: Option<String>,
    /// `low`, `normal` (the default) or `high`.
    priority: Option<Priority>,
}

backed! {
    /// Sends one notification.
    NotifyTool over Notifier
}

impl Handler for NotifyTool<'_> {
    const NAME: &'static str = "notify";
    const DESCRIPTION: &'static str = "Send a push notification to the person this run works for.";
    type Arguments = Notify;

    async fn run(&self, arguments: Notify, context: Context<'_>) -> ToolOutput {
        let note = Notification {
            message: &arguments.message,
            title: arguments.title.as_deref(),
            priority: arguments.priority.unwrap_or_default(),
        };
        match self.backend.notify(context.call_ref, note).await {
            Ok(text) => ToolOutput::succeeded(text),
            Err(refusal) => refusal.into(),
        }
    }
}

#[cfg(test)]
mod tests;
