//! Fixed routing text uses a stable channel carrier and the ordinary outbound ledger.
//! A carrier is never an admission target and never creates or rewrites a fleet.

use af_core::id::Uuid7;
use afd_api_wire::ingress::Accepted;
use afd_connector::Provider;
use afd_ingress::slack::{Notice, NoticeOwed, notice_text};
use std::borrow::Cow;

use super::{
    Asked, EVENT_MENTION, EVENT_ROUTED, Outcome, REASON_CHANNEL_NOT_CONFIGURED,
    REASON_NOTICE_OWNER_KILLED, address,
};
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
) -> Result<Outcome, Refusal> {
    let Some(owner) = owner(services, provider, workspace, asked, notice).await? else {
        return Ok(Outcome::Dropped(REASON_CHANNEL_NOT_CONFIGURED));
    };
    if owner.status == FleetStatus::Killed {
        return Ok(Outcome::Dropped(REASON_NOTICE_OWNER_KILLED));
    }
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
    notice: &Notice<'_>,
) -> Result<Option<NoticeOwner>, Refusal> {
    let ingress = services.ingress();
    let existing = ingress
        .notice_owner(workspace, provider.id(), &asked.team_id, &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    if existing.is_some() {
        return Ok(existing);
    }
    let Some(candidate) = candidate(notice) else {
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

fn candidate<'a>(notice: &'a Notice<'a>) -> Option<&'a Subscriber> {
    match notice {
        Notice::Paused { fleet } => Some(*fleet),
        Notice::Ambiguous { fleets } | Notice::Choose { fleets } | Notice::AddressIt { fleets } => {
            fleets
                .iter()
                .copied()
                .min_by(|left, right| left.fleet.as_str().cmp(right.fleet.as_str()))
        }
    }
}
