//! Fixed routing text uses a stable channel carrier and the ordinary outbound ledger.
//! A carrier is never an admission target and never creates or rewrites a fleet.

use af_core::id::Uuid7;
use afd_api_wire::ingress::Accepted;
use afd_connector::Provider;
use afd_ingress::slack::{Notice, NoticeOwed, notice_text};
use std::borrow::Cow;

use super::{Asked, EVENT_MENTION, EVENT_ROUTED, Outcome, REASON_NO_ACTIVE_CARRIER, address};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};
use afd_fleet_lifecycle::FleetStatus;
use afd_ingress::slack::{NoticeOwner, Subscriber};

/// Owes `notice` to the thread `asked` was asked in.
///
/// # Errors
/// A datastore or a ledger that would not answer, as the refusal a
/// provider retries.
pub(super) async fn owe<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
    notice: &Notice<'_>,
    subscribers: &[Subscriber],
) -> Result<Outcome, Refusal> {
    let owner = owner(services, provider, workspace, asked, subscribers).await?;
    let Some(owner) = owner.filter(|owner| owner.status != FleetStatus::Archived) else {
        return Ok(Outcome::Dropped(REASON_NO_ACTIVE_CARRIER));
    };
    let address = address(&asked.thread())?;
    let text = notice_text(notice);
    let owed = services
        .ingress()
        .owe_notice(
            NoticeOwed {
                owner: &owner.fleet,
                workspace,
                provider,
                address: &address,
                text: &text,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;

    Ok(accepted(workspace, &owner.fleet, notice.kind(), owed))
}

fn accepted(workspace: &Uuid7, owner: &Uuid7, verdict: &str, owed: Uuid7) -> Outcome {
    let workspace_id = workspace.as_str();
    let fleet_id = owner.as_str();
    tracing::info!(workspace_id, fleet_id, verdict, event = EVENT_ROUTED);
    Outcome::Accepted(Accepted {
        event_id: Cow::Owned(owed.into()),
        replayed: false,
    })
}

/// Existing bindings, including historical ones, are notice carriers only.
async fn owner<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
    subscribers: &[Subscriber],
) -> Result<Option<NoticeOwner>, Refusal> {
    let ingress = services.ingress();
    let existing = ingress
        .notice_owner(workspace, provider.id(), &asked.team_id, &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    if existing.is_some() {
        return Ok(existing);
    }
    let Some(candidate) = carrier(subscribers) else {
        return Ok(None);
    };
    ingress
        .bind_notice_owner(
            workspace,
            provider.id(),
            &asked.team_id,
            &asked.channel,
            &candidate.fleet,
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    // The channel's unique key chooses one owner even across concurrent first notices.
    ingress
        .notice_owner(workspace, provider.id(), &asked.team_id, &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?
        .map(Some)
        .ok_or_else(|| {
            Refusal::coded(
                af_core::error_code::INTERNAL_DB_UNAVAILABLE,
                af_core::error::DETAIL_DATABASE_UNAVAILABLE,
            )
        })
}

/// The channel's carrier: its oldest runnable fleet, whatever the notice is
/// about. An archived fleet is never chosen, since it could not carry the
/// obligation, so a channel with no runnable fleet has no carrier.
fn carrier(subscribers: &[Subscriber]) -> Option<&Subscriber> {
    subscribers
        .iter()
        .filter(|subscriber| subscriber.runnable)
        .min_by(|left, right| left.fleet.as_str().cmp(right.fleet.as_str()))
}
