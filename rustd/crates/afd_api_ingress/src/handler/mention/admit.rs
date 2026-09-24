//! A mention routing gave one fleet: its thread read back, the fleet's message
//! composed, one admission recorded.
//!
//! Split from `mention.rs`, which parses, resolves and routes.
//!
//! # No pool connection rides the thread read
//!
//! The read waits on Slack for up to its deadline. Everything this path read
//! from the database before it — the workspace, the bot's token, the
//! subscribers — was read and its connection returned by the time routing
//! answered, and the admission acquires its own afterwards. What reaches the
//! read is a token, not a workspace, so the seam has nothing to open a
//! connection with (RULE CNX).
//!
//! # A failed read never refuses the mention
//!
//! The person asked something; the fleet can still answer it without the
//! thread. A read that timed out or that Slack refused leaves one line naming
//! why in the thread's place, and the admission goes ahead.

use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretString;
use afd_ingress::slack::{MentionAdmission, Subscriber, compose};
use afd_wire::ingress::{Accepted, MentionRequest, MentionRoute};
use std::borrow::Cow;

use super::{Asked, EVENT_MENTION, EVENT_ROUTED, Outcome, address, unserialisable};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _, WorkspaceConnectors as _};

/// A mention routing gave one fleet, with what admitting it reads.
pub(super) struct Routed<'a> {
    /// The workspace the team resolved to.
    pub(super) workspace: &'a Uuid7,
    /// The mention as parsed.
    pub(super) asked: &'a Asked,
    /// The bot's token, already loaded, which the thread read spends.
    pub(super) token: &'a SecretString,
    /// The fleet routing chose.
    pub(super) fleet: &'a Subscriber,
    /// What was asked, with the bot and any addressed name removed.
    pub(super) message: &'a str,
    /// How routing chose the fleet.
    pub(super) verdict: &'static str,
}

/// Reads the thread back, composes the fleet's message and admits it.
///
/// # Errors
/// A datastore that would not take the admission, as the 503 a provider
/// retries. A thread that could not be read is not an error.
pub(super) async fn routed<D: Services>(
    services: &D,
    provider: Provider,
    routed: Routed<'_>,
) -> Result<Outcome, Refusal> {
    let Routed {
        workspace,
        asked,
        token,
        fleet,
        message,
        verdict,
    } = routed;
    let thread = asked.thread();
    let address = address(&thread)?;

    let read = services.connectors().thread(token, &thread).await;
    let composed = compose(message, &asked.ts, &read);
    let body = MentionRequest {
        message: Cow::Borrowed(&composed.message),
        channel_id: Cow::Borrowed(asked.channel.as_str()),
        reply_thread_ts: Cow::Borrowed(&asked.thread_ts),
        route: MentionRoute {
            verdict: Cow::Borrowed(verdict),
            fleet: Cow::Borrowed(&fleet.name),
        },
        thread: composed.thread,
    };
    let request_json = serde_json::to_string(&body).map_err(|_unserialisable| unserialisable())?;

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
    let thread_unavailable = read.as_ref().err().map(|reason| reason.as_str());
    tracing::info!(
        workspace_id,
        fleet_id,
        verdict,
        event_id,
        replayed = admitted.replayed,
        thread_fetched = body.thread.fetched,
        message_count = body.thread.count,
        thread_unavailable,
        event = EVENT_ROUTED,
    );
    Ok(Outcome::Accepted(Accepted {
        event_id: Cow::Owned(admitted.id),
        replayed: admitted.replayed,
    }))
}
