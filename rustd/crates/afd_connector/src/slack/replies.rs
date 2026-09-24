//! Reading a Slack thread back, for the fleet a mention woke.
//!
//! `conversations.replies` answers a thread oldest first, parent included, a
//! page at a time. The fleet needs the parent — in an incident channel that is
//! the CI announcement carrying the run link — and the latest replies, which
//! is what the people in the thread have said since. So every page is read and
//! a bounded window keeps the parent and the newest [`MAX_MESSAGES`]` - 1`
//! replies; a thread of any length costs the same memory.
//!
//! # One deadline for the whole read
//!
//! The mention is still waiting on Slack's three-second delivery window when
//! this runs, so the read is bounded as a whole by [`READ_DEADLINE`] rather
//! than per request, where a many-page thread would multiply it. A read that
//! misses the deadline, or that Slack refuses, is an [`Unavailable`] reason,
//! never an error: the mention is admitted without the thread either way.
//!
//! # No pool connection rides the vendor call
//!
//! Nothing here takes a database handle. The caller has loaded the token and
//! returned its connection before this is entered, and the signature is what
//! enforces it.
//!
//! # Where it dials
//!
//! Slack's host, or the origin a lane pinned the exchange at — the one knob
//! [`crate::endpoint`] derives every vendor host from — so a test's thread read
//! lands on its loopback and never on Slack.

use std::ops::ControlFlow;
use std::time::Duration;

use afd_crypto::secret::SecretString;
use serde::Deserialize;
use serde_json::Value;

use self::shown::{Window, flatten};
use super::answered::MessageMetadata;
use super::{SLACK_API_BASE, Thread};
use crate::connect::Connectors;
use crate::endpoint;
use crate::error::Result;

/// Slack's method for one thread's messages, parent first.
pub(super) const METHOD_CONVERSATIONS_REPLIES: &str = "/conversations.replies";

/// How long the whole read may take, every page included.
pub const READ_DEADLINE: Duration = Duration::from_millis(1_500);

/// The most messages one read keeps: the parent and the latest replies.
pub const MAX_MESSAGES: usize = 20;

/// How many messages one page asks for; Slack advises no more than 200.
const PAGE_LIMIT: &str = "200";

/// The form fields one page is asked with, named once each (RULE UFS).
const FIELD_CHANNEL: &str = "channel";
/// See [`FIELD_CHANNEL`].
const FIELD_TS: &str = "ts";
/// See [`FIELD_CHANNEL`].
const FIELD_LIMIT: &str = "limit";
/// See [`FIELD_CHANNEL`].
const FIELD_CURSOR: &str = "cursor";
/// See [`FIELD_CHANNEL`]. Asked on every page, so the answer check reads the
/// marker a posted answer carries; the mention's read ignores it.
const FIELD_INCLUDE_METADATA: &str = "include_all_metadata";
/// The value a boolean form field is sent as.
const FORM_TRUE: &str = "true";

/// Why a thread could not be read, spelled once for the fleet and the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// [`READ_DEADLINE`] fired before every page arrived.
    Timeout,
    /// Slack answered `ok: false`: a channel the bot is not in, a missing scope.
    Refused,
    /// Slack answered with a status other than 200.
    Status,
    /// Slack could not be reached, or a lane's pin is not a usable origin.
    Unreachable,
    /// A 200 whose body is not Slack's answer.
    Unreadable,
}

impl Unavailable {
    /// The reason as the fleet's message and the log spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Refused => "refused",
            Self::Status => "unexpected_status",
            Self::Unreachable => "unreachable",
            Self::Unreadable => "unreadable",
        }
    }
}

/// One message of a thread, flattened to what a person reading it sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Slack's own id for the message.
    pub ts: String,
    /// The person or bot that posted it; empty when Slack named neither.
    pub author: String,
    /// The text, then every attachment and block string, one per line.
    pub text: String,
}

/// A thread as read: the parent, then the latest replies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replies {
    /// At most [`MAX_MESSAGES`], oldest first.
    pub messages: Vec<Message>,
    /// Every message the thread held, kept or not.
    pub seen: usize,
}

/// One page of Slack's answer. Every field defaults, so a 200 that is JSON
/// but not Slack's reads as refused rather than as an empty thread.
#[derive(Debug, Default, Deserialize)]
struct Page {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    messages: Vec<Posted>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    response_metadata: Metadata,
}

/// Where the next page starts.
#[derive(Debug, Default, Deserialize)]
struct Metadata {
    #[serde(default)]
    next_cursor: String,
}

/// One message as Slack renders it.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Posted {
    #[serde(default)]
    ts: String,
    user: Option<String>,
    bot_id: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    attachments: Vec<Value>,
    #[serde(default)]
    blocks: Vec<Value>,
    /// The metadata an app posted the message with, if any.
    #[serde(default)]
    pub(super) metadata: Option<MessageMetadata>,
}

impl Connectors {
    /// Reads `thread` back with the bot's `token`, on this flow's vendor client
    /// and at the origin its exchange was pinned to.
    ///
    /// # Errors
    /// The [`Unavailable`] reason the read failed for.
    pub async fn thread(
        &self,
        token: &SecretString,
        thread: &Thread,
    ) -> Result<Replies, Unavailable> {
        replies(&self.client, self.exchange.pinned_endpoint(), token, thread).await
    }
}

/// Reads `thread`: every page, under [`READ_DEADLINE`].
///
/// # Errors
/// The [`Unavailable`] reason the read failed for. A pin that is not a usable
/// origin refuses before anything is dialled rather than falling back to
/// Slack — the lane set it to keep this bearer off the vendor.
pub async fn replies(
    client: &reqwest::Client,
    pinned: Option<&str>,
    token: &SecretString,
    thread: &Thread,
) -> Result<Replies, Unavailable> {
    let vendor = format!("{SLACK_API_BASE}{METHOD_CONVERSATIONS_REPLIES}");
    let endpoint = endpoint::redirected(&vendor, pinned).ok_or(Unavailable::Unreachable)?;
    tokio::time::timeout(
        READ_DEADLINE,
        pages(client, &endpoint, token.expose(), thread),
    )
    .await
    .unwrap_or(Err(Unavailable::Timeout))
}

/// Every page of the thread, folded into the parent and the latest replies.
async fn pages(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    thread: &Thread,
) -> Result<Replies, Unavailable> {
    let mut window = Window::default();
    walk(client, endpoint, token, thread, |posted| {
        window.push(flatten(posted));
        ControlFlow::Continue(())
    })
    .await?;
    Ok(window.into_replies())
}

/// Hands every message of the thread to `visit`, oldest first and a page at
/// a time, until the thread ends or `visit` breaks.
///
/// The one cursor loop both readers share: the mention's, which keeps a
/// window, and the answer check, which stops at the first marker it knows.
pub(super) async fn walk(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    thread: &Thread,
    mut visit: impl FnMut(Posted) -> ControlFlow<()>,
) -> Result<(), Unavailable> {
    let mut cursor = String::new();
    loop {
        let page = page(client, endpoint, token, thread, &cursor).await?;
        if page
            .messages
            .into_iter()
            .map(&mut visit)
            .any(|flow| flow.is_break())
        {
            return Ok(());
        }
        let next = page.response_metadata.next_cursor;
        if !page.has_more || next.is_empty() {
            return Ok(());
        }
        cursor = next;
    }
}

/// One page, starting at `cursor` (empty for the first).
async fn page(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    thread: &Thread,
    cursor: &str,
) -> Result<Page, Unavailable> {
    let mut form = vec![
        (FIELD_CHANNEL, thread.channel_id.as_str()),
        (FIELD_TS, thread.thread_ts.as_str()),
        (FIELD_LIMIT, PAGE_LIMIT),
        (FIELD_INCLUDE_METADATA, FORM_TRUE),
    ];
    if !cursor.is_empty() {
        form.push((FIELD_CURSOR, cursor));
    }
    let response = client
        .post(endpoint)
        .bearer_auth(token)
        .form(&form)
        .send()
        .await
        .map_err(|_transport| Unavailable::Unreachable)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(Unavailable::Status);
    }
    let body = response
        .text()
        .await
        .map_err(|_cut_short| Unavailable::Unreachable)?;
    let page: Page = serde_json::from_str(&body).map_err(|_not_json| Unavailable::Unreadable)?;
    if page.ok {
        Ok(page)
    } else {
        Err(Unavailable::Refused)
    }
}

#[path = "replies/shown.rs"]
mod shown;

#[cfg(test)]
#[path = "replies/tests.rs"]
mod tests;
