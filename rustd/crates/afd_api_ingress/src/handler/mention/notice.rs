//! A mention routing gave no fleet: fixed text owed to its thread.
//!
//! Split from `mention.rs`, which routes. No model runs and no event is
//! admitted. The channel's resident owns the notice, so it is found or
//! installed first, and the notice is owed under a key derived from the chat
//! event's own id: a retried delivery finds it already owed and owes nothing
//! more.
//!
//! The route answers as it does for an admitted mention, with the notice's key
//! where an event id would be, so a delivery that owed nothing new reads as
//! replayed.

use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_ingress::slack::{Notice, NoticeOwed, notice_key, notice_text};
use afd_wire::ingress::Accepted;
use std::borrow::Cow;

use super::resident::resident;
use super::{Asked, EVENT_MENTION, EVENT_ROUTED, Outcome, address};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};

/// Owes `notice` to the thread `asked` was asked in.
///
/// # Errors
/// A datastore, an install or a ledger that would not answer, as the refusal a
/// provider retries.
pub(super) async fn owe<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
    notice: &Notice<'_>,
) -> Result<Outcome, Refusal> {
    let owner = match resident(services, provider, workspace, asked)
        .await?
        .resident()
    {
        Ok(owner) => owner,
        Err(settled) => return Ok(settled),
    };
    let address = address(&asked.thread())?;
    let key = notice_key(&asked.team_id, &asked.event_id);
    let text = notice_text(notice);
    let written = services
        .ingress()
        .owe_notice(
            NoticeOwed {
                resident: &owner.fleet,
                workspace,
                provider,
                key: &key,
                address: &address,
                text: &text,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;

    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let workspace_id = workspace.as_str();
    let fleet_id = owner.fleet.as_str();
    let verdict = notice.kind();
    let replayed = !written;
    tracing::info!(
        workspace_id,
        fleet_id,
        verdict,
        replayed,
        event = EVENT_ROUTED
    );
    Ok(Outcome::Accepted(Accepted {
        event_id: Cow::Owned(key),
        replayed,
    }))
}
