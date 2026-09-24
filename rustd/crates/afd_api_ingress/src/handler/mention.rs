//! A verified chat mention: parsed, filtered, resolved, routed and admitted.
//!
//! Split from `events.rs`, which keeps the wall and the handshake. Everything
//! here runs on a body [`super::webhook::verified_connector_events`] already
//! proved, and every outcome is a 200 — a drop with its reason, or the event
//! the mention woke — except a datastore that would not answer, which is a 503
//! so the provider retries.
//!
//! ```text
//!   envelope ─► parse+filter ─► team→workspace ─► bot identity ─► subscribers
//!                  │                  │                 │               │
//!                  └ unreadable_body  └ team_not_mapped └ bot_message   ▼
//!                    bot_message                                       route ─► admit
//!                    unsupported_event
//! ```
//!
//! The envelope is a `serde`-tagged enum, so "an `event_callback` whose event
//! is an `app_mention`" is a type rather than three field lookups that have to
//! agree (RULE PSR).

use afd_connector::Provider;
use afd_connector::slack::Thread;
use afd_core::id::Uuid7;
use afd_ingress::slack::{ChannelId, MentionAdmission, Route, Subscriber, route};
use afd_wire::ingress::{Accepted, MentionRequest, MentionRoute, MentionThread};
use serde::Deserialize;
use std::borrow::Cow;

use super::events::REASON_UNREADABLE;
use super::webhook::REASON_UNSUPPORTED_EVENT;
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _, WorkspaceConnectors as _};

/// Why a mention was dropped, named once each (RULE UFS). `unreadable_body`
/// is the route's own, shared with a body that is not JSON at all, and
/// `unsupported_event` is every signed route's, for a kind no rule serves.
pub(super) const REASON_BOT_MESSAGE: &str = "bot_message";
/// See [`REASON_BOT_MESSAGE`].
pub(super) const REASON_TEAM_NOT_MAPPED: &str = "team_not_mapped";

/// The event a routed mention is logged under.
const EVENT_ROUTED: &str = "slack_mention_routed";
/// The event a datastore failure on this path is refused under.
const EVENT_MENTION: &str = "slack_mention_failed";

/// How a verdict is spelled in the event body and the log.
const VERDICT_ADDRESSED: &str = "addressed";
/// See [`VERDICT_ADDRESSED`].
const VERDICT_SOLE: &str = "sole";

/// The opening of a user mention in Slack's message markup: `<@U0123>`.
const MENTION_OPEN: &str = "<@";
/// Its close.
const MENTION_CLOSE: char = '>';

/// The delivery, as Slack's Events API wraps it.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Envelope {
    /// A subscribed event, with the team and Slack's own id for the delivery.
    EventCallback {
        team_id: String,
        event_id: String,
        event: Event,
    },
    /// Any other envelope; the handshake is answered before parsing reaches
    /// here.
    #[serde(other)]
    Other,
}

/// The event inside an `event_callback`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    /// The bot was mentioned. Boxed: its seven strings would otherwise size
    /// every envelope, including the ones that are dropped unread.
    AppMention(Box<AppMention>),
    /// An event this route does not act on.
    #[serde(other)]
    Other,
}

/// An `app_mention`, with every field optional that Slack omits for bots.
#[derive(Debug, Deserialize)]
struct AppMention {
    user: Option<String>,
    #[serde(default)]
    text: String,
    ts: String,
    thread_ts: Option<String>,
    channel: String,
    bot_id: Option<String>,
    subtype: Option<String>,
}

/// A mention a person made, past every drop that needs no datastore.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Asked {
    pub(super) team_id: String,
    pub(super) event_id: String,
    pub(super) user: String,
    pub(super) text: String,
    pub(super) channel: ChannelId,
    /// The thread's root: the mention's own thread, or the mention itself
    /// when it started one.
    pub(super) thread_ts: String,
}

/// What a verified envelope is, before any datastore is asked.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Parsed {
    /// A person's mention in a channel.
    Asked(Asked),
    /// Acknowledged and dropped, with the reason.
    Dropped(&'static str),
}

/// What admitting a parsed mention came to.
#[derive(Debug)]
pub(super) enum Outcome {
    /// The event the mention woke.
    Accepted(Accepted<'static>),
    /// Acknowledged and dropped, with the reason.
    Dropped(&'static str),
}

/// Parses and filters one verified envelope.
///
/// A message carrying `bot_id` or a `subtype` (an edit, a join, a bot post) is
/// `bot_message`; a mention with no user, or a body missing a field Slack
/// always sends, is `unreadable_body`; a direct message's channel, or any
/// other event, is `unsupported_event`.
pub(super) fn parse(envelope: serde_json::Value) -> Parsed {
    let Ok(envelope) = serde_json::from_value::<Envelope>(envelope) else {
        return Parsed::Dropped(REASON_UNREADABLE);
    };
    let Envelope::EventCallback {
        team_id,
        event_id,
        event: Event::AppMention(mention),
    } = envelope
    else {
        return Parsed::Dropped(REASON_UNSUPPORTED_EVENT);
    };
    if mention.bot_id.is_some() || mention.subtype.is_some() {
        return Parsed::Dropped(REASON_BOT_MESSAGE);
    }
    let Some(user) = mention.user.filter(|user| !user.is_empty()) else {
        return Parsed::Dropped(REASON_UNREADABLE);
    };
    if team_id.is_empty() || event_id.is_empty() || mention.ts.is_empty() {
        return Parsed::Dropped(REASON_UNREADABLE);
    }
    let Ok(channel) = mention.channel.parse::<ChannelId>() else {
        return Parsed::Dropped(REASON_UNSUPPORTED_EVENT);
    };
    Parsed::Asked(Asked {
        team_id,
        event_id,
        user,
        text: mention.text,
        channel,
        thread_ts: mention.thread_ts.unwrap_or(mention.ts),
    })
}

/// The mention's text with the leading bot mention removed.
///
/// An `app_mention` starts with the bot's own `<@U…>`; whatever follows is
/// what the person asked.
pub(super) fn without_bot_mention(text: &str) -> &str {
    let text = text.trim_start();
    text.strip_prefix(MENTION_OPEN)
        .and_then(|rest| rest.split_once(MENTION_CLOSE))
        .map_or(text, |(_bot, asked)| asked.trim_start())
}

/// Resolves, routes and admits one parsed mention.
///
/// # Errors
/// A datastore that would not answer at any step, as the 503 a provider
/// retries.
pub(super) async fn admit<D: Services>(
    services: &D,
    provider: Provider,
    asked: &Asked,
) -> Result<Outcome, Refusal> {
    let Some(workspace) = services
        .ingress()
        .installation_workspace(provider.id(), &asked.team_id)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?
    else {
        return Ok(Outcome::Dropped(REASON_TEAM_NOT_MAPPED));
    };
    // A mapped team whose grant holds no bot cannot answer as itself, which
    // is the same fact as an unmapped one for the person waiting.
    let Some(identity) = services
        .connectors()
        .bot_identity(&workspace, provider)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?
    else {
        return Ok(Outcome::Dropped(REASON_TEAM_NOT_MAPPED));
    };
    if identity.user_id.as_deref() == Some(asked.user.as_str()) {
        return Ok(Outcome::Dropped(REASON_BOT_MESSAGE));
    }

    let subscribers = services
        .ingress()
        .mention_subscribers(&workspace, provider.id(), &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    let (fleet, message, verdict) = match route(&subscribers, without_bot_mention(&asked.text)) {
        Route::Addressed { fleet, message } => (fleet, message, VERDICT_ADDRESSED),
        Route::Sole { fleet, message } => (fleet, message, VERDICT_SOLE),
        // The resident and the notices land in their own sections; until
        // then a mention nobody can take is acknowledged, not run.
        Route::Resident { .. } | Route::Notice(_) => {
            return Ok(Outcome::Dropped(REASON_UNSUPPORTED_EVENT));
        }
    };
    admit_routed(
        services, provider, &workspace, asked, fleet, message, verdict,
    )
    .await
}

/// Admits a mention routing gave one fleet.
async fn admit_routed<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
    fleet: &Subscriber,
    message: &str,
    verdict: &'static str,
) -> Result<Outcome, Refusal> {
    let thread = Thread {
        team_id: Some(asked.team_id.clone()),
        channel_id: asked.channel.as_str().to_owned(),
        thread_ts: asked.thread_ts.clone(),
    };
    let address = thread.address().map_err(|_unserialisable| {
        Refusal::coded(
            afd_core::error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_UNSERIALISABLE,
        )
    })?;
    let body = MentionRequest {
        message: Cow::Borrowed(message),
        channel_id: Cow::Borrowed(asked.channel.as_str()),
        reply_thread_ts: Cow::Borrowed(&asked.thread_ts),
        route: MentionRoute {
            verdict: Cow::Borrowed(verdict),
            fleet: Cow::Borrowed(&fleet.name),
        },
        thread: MentionThread {
            fetched: false,
            count: 0,
            truncated: false,
        },
    };
    let request_json = serde_json::to_string(&body).map_err(|_unserialisable| {
        Refusal::coded(
            afd_core::error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_UNSERIALISABLE,
        )
    })?;

    let admitted = services
        .ingress()
        .admit_mention(MentionAdmission {
            fleet: &fleet.fleet,
            workspace,
            team_id: &asked.team_id,
            event_id: &asked.event_id,
            user: &asked.user,
            request_json: &request_json,
            connector: provider.id(),
            address: &address,
        })
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;

    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let workspace_id = workspace.as_str();
    let fleet_id = fleet.fleet.as_str();
    let event_id = admitted.id.as_str();
    tracing::info!(
        workspace_id,
        fleet_id,
        verdict,
        event_id,
        replayed = admitted.replayed,
        thread_fetched = body.thread.fetched,
        message_count = body.thread.count,
        event = EVENT_ROUTED,
    );
    Ok(Outcome::Accepted(Accepted {
        event_id: Cow::Owned(admitted.id),
        replayed: admitted.replayed,
    }))
}

/// The detail a body this daemon could not serialise is refused with.
const DETAIL_UNSERIALISABLE: &str = "The mention could not be recorded.";

#[cfg(test)]
#[path = "mention/tests.rs"]
mod tests;
