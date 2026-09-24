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

use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{Install, LibrarySource};
use afd_ingress::slack::{Resident, Subscriber};

use super::{Asked, EVENT_MENTION};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _, WorkspaceFleets as _};

/// The event a newly installed resident is logged under.
const EVENT_MATERIALIZED: &str = "slack_resident_materialized";

/// The detail a race whose winner rolled back is refused with. A retry finds
/// no name taken and installs.
const DETAIL_RESIDENT_RACE: &str = afd_core::error::DETAIL_DATABASE_UNAVAILABLE;

/// The resident `asked`'s channel answers with, installing it on the first
/// mention.
///
/// `None` when the team's identifier cannot form a fleet name, which the caller
/// drops as a mention it cannot read.
///
/// # Errors
/// A datastore or an install that would not answer, as the refusal a provider
/// retries.
pub(super) async fn resident<D: Services>(
    services: &D,
    provider: Provider,
    workspace: &Uuid7,
    asked: &Asked,
) -> Result<Option<Subscriber>, Refusal> {
    let Some(resident) = Resident::for_channel(&asked.team_id, &asked.channel) else {
        return Ok(None);
    };
    let bound = services
        .ingress()
        .resident(provider.id(), &asked.team_id, &asked.channel)
        .await
        .map_err(Refusal::at(EVENT_MENTION))?;
    let fleet = if let Some(fleet) = bound {
        fleet
    } else {
        let installed = materialise(services, workspace, &resident).await?;
        services
            .ingress()
            .bind_resident(
                provider.id(),
                &asked.team_id,
                &asked.channel,
                &installed,
                services.now(),
            )
            .await
            .map_err(Refusal::at(EVENT_MENTION))?
    };
    Ok(Some(Subscriber {
        fleet,
        name: resident.name.as_str().to_owned(),
        runnable: true,
        addressed_only: false,
    }))
}

/// Installs `resident`, or finds the one a concurrent first mention installed.
async fn materialise<D: Services>(
    services: &D,
    workspace: &Uuid7,
    resident: &Resident,
) -> Result<Uuid7, Refusal> {
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
            Ok(installed.id)
        }
        Err(lost) if lost.is_name_taken() => services
            .ingress()
            .fleet_named(workspace, &resident.name)
            .await
            .map_err(Refusal::at(EVENT_MENTION))?
            .ok_or_else(|| {
                Refusal::coded(
                    afd_core::error_code::INTERNAL_DB_UNAVAILABLE,
                    DETAIL_RESIDENT_RACE,
                )
            }),
        Err(failed) => Err(Refusal::at(EVENT_MENTION)(failed)),
    }
}
