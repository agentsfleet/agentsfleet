//! The channel's resident, found or materialised for a mention no attached
//! fleet takes.
//!
//! Split from `mention.rs`, which routes. The resident is installed through
//! the one path every fleet is installed through, with a bundle written in
//! code (`afd_ingress::slack::Resident`), and bound to the channel so the next
//! mention finds it without installing again.
//!
//! # Two first mentions at once
//!
//! Both miss the binding and both install under the channel's one name. The
//! workspace holds a single fleet per name, so one install wins and the other
//! is refused as a taken name; the loser finds the winner by that name. The
//! binding is insert-once, so whichever of them binds first, both read back
//! the same fleet (RULE IDMP).
//!
//! # A resident that cannot run
//!
//! A stopped resident answers with the paused notice and its resume command,
//! as an attached fleet that is paused does; admitting onto it would leave the
//! person with no answer at all. A killed one is on its way to deletion, which
//! takes the binding with it, so its mention is dropped until then.

use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{FleetStatus, Install, LibrarySource};
use afd_ingress::slack::{BoundResident, Named, Notice, Resident, Subscriber};

use super::{
    Asked, EVENT_MENTION, Outcome, REASON_RESIDENT_KILLED, REASON_RESIDENT_NAME_TAKEN, notice,
};
use crate::handler::Refusal;
use crate::handler::events::REASON_UNREADABLE;
use crate::services::{Services, WebhookIngress as _, WorkspaceFleets as _};

/// The event a newly installed resident is logged under.
const EVENT_MATERIALIZED: &str = "slack_resident_materialized";

/// The detail a race whose winner rolled back is refused with. A retry finds
/// no name taken and installs.
const DETAIL_RESIDENT_RACE: &str = afd_core::error::DETAIL_DATABASE_UNAVAILABLE;

/// What a mention routed to the channel's resident comes to.
pub(super) enum Answering {
    /// The resident runs it.
    Resident(Subscriber),
    /// Settled without a run: dropped, or answered with a notice.
    Settled(Outcome),
}

/// The channel's resident, or where a mention for it goes instead.
pub(super) enum Found {
    /// Bound in the mention's workspace, installed first when it was not.
    Resident(Subscriber),
    /// The team's identifier cannot form a fleet name.
    Unnamed,
    /// Bound, and killed.
    Killed,
    /// A fleet that is not the resident already holds the resident's name.
    NameTaken,
}

impl Found {
    /// The outcome a mention whose resident will not run comes to, or the
    /// resident.
    pub(super) fn resident(self) -> Result<Subscriber, Outcome> {
        match self {
            Self::Resident(found) => Ok(found),
            Self::Unnamed => Err(Outcome::Dropped(REASON_UNREADABLE)),
            Self::Killed => Err(Outcome::Dropped(REASON_RESIDENT_KILLED)),
            Self::NameTaken => Err(Outcome::Dropped(REASON_RESIDENT_NAME_TAKEN)),
        }
    }
}

/// Runs a mention on the channel's resident when it can run, and answers the
/// paused notice when it cannot.
///
/// # Errors
/// A datastore, an install or a ledger that would not answer, as the refusal a
/// provider retries.
pub(super) async fn answering<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
) -> Result<Answering, Refusal> {
    let found = match resident(services, provider, workspace, asked)
        .await?
        .resident()
    {
        Ok(found) if found.runnable => return Ok(Answering::Resident(found)),
        Ok(found) => found,
        Err(settled) => return Ok(Answering::Settled(settled)),
    };
    let paused = Notice::Paused { fleet: &found };
    notice::owe(services, provider, workspace, asked, &paused)
        .await
        .map(Answering::Settled)
}

/// The resident `asked`'s channel answers with in `workspace`, installing it
/// on the first mention there.
///
/// # Errors
/// A datastore or an install that would not answer, as the refusal a provider
/// retries.
pub(super) async fn resident<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
) -> Result<Found, Refusal> {
    let Some(resident) = Resident::for_channel(&asked.team_id, &asked.channel) else {
        return Ok(Found::Unnamed);
    };
    let bound = services
        .ingress()
        .resident(workspace, provider.id(), &asked.team_id, &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    let (fleet, status) = if let Some(bound) = bound {
        (bound.fleet, bound.status)
    } else {
        let Some(installed) = materialise(services, workspace, &resident).await? else {
            return Ok(Found::NameTaken);
        };
        let fleet = services
            .ingress()
            .bind_resident(
                workspace,
                provider.id(),
                &asked.team_id,
                &asked.channel,
                &installed.fleet,
                services.now(),
            )
            .await
            .map_err(Refusal::at(EVENT_MENTION))?;
        (fleet, installed.status)
    };
    if status == FleetStatus::Killed {
        return Ok(Found::Killed);
    }
    Ok(Found::Resident(Subscriber {
        fleet,
        name: resident.name.as_str().to_owned(),
        runnable: status == FleetStatus::Active,
        addressed_only: false,
    }))
}

/// Installs `resident`, or finds the one a concurrent first mention installed.
///
/// `None` when the name is held by a fleet that is not the resident: its
/// configuration is somebody else's, and adopting it would hand every
/// unaddressed mention to whatever that fleet may do.
async fn materialise<D: Services>(
    services: &D,
    workspace: &Uuid7,
    resident: &Resident,
) -> Result<Option<BoundResident>, Refusal> {
    let install = Install {
        source: LibrarySource::InCode {
            skill_markdown: &resident.skill_markdown,
            trigger_markdown: &resident.trigger_markdown,
        },
        name: Some(resident.name.clone()),
        mention: None,
    };
    match services
        .fleets()
        .install(workspace, &install, services.now())
        .await
    {
        Ok(installed) => {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let workspace_id = workspace.as_str();
            let fleet_id = installed.id.as_str();
            tracing::info!(workspace_id, fleet_id, event = EVENT_MATERIALIZED);
            Ok(Some(BoundResident {
                fleet: installed.id,
                status: FleetStatus::Active,
            }))
        }
        Err(lost) if lost.is_name_taken() => match services
            .ingress()
            .resident_named(workspace, resident)
            .await
            .map_err(Refusal::at(EVENT_MENTION))?
        {
            Some(Named::Resident(found)) => Ok(Some(found)),
            Some(Named::Other) => Ok(None),
            None => Err(Refusal::coded(
                afd_core::error_code::INTERNAL_DB_UNAVAILABLE,
                DETAIL_RESIDENT_RACE,
            )),
        },
        Err(failed) => Err(Refusal::at(EVENT_MENTION)(failed)),
    }
}
