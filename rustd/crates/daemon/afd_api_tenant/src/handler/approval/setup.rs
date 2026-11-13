//! What an approved setup grant sets moving: the fleet's setup, then, once it
//! is active, its trigger schedules.
//!
//! Approving the last missing grant is the person's whole answer, so the fleet
//! activates without a second request, through the same two steps PATCH
//! `active`, Save and revision apply run. The decision is already committed
//! and is the caller's answer, so a step that fails here is logged and never
//! turned into a refusal of the approval; the next approval, Save or PATCH
//! `active` retries it.

use std::sync::Arc;

use af_core::id::Uuid7;
use afd_approval::{Decision, GateRow, KIND_INTEGRATION_GRANT, Resolution};

use crate::auth::PersonIdentity;
use crate::handler::fleet::native::schedules::after_activation;
use crate::services::Services;

/// An approved setup grant whose setup step then failed.
const EVENT_SETUP_RESUME_FAILED: &str = "approval_setup_resume_failed";
/// A fleet an approval activated whose schedules could not then be read.
const EVENT_SCHEDULES_UNSYNCED: &str = "approval_setup_schedules_unsynced";
/// The stored status of a fleet that takes work.
const ACTIVE: &str = "active";

/// Resumes setup when `resolution` is this request's own approval of a setup
/// grant; every other answer changes nothing here.
pub(super) async fn after_resolution<D: Services>(
    services: &Arc<D>,
    gate: &GateRow,
    outcome: Decision,
    resolution: &Resolution,
    person: &PersonIdentity,
) {
    if matches!(resolution, Resolution::Resolved(_))
        && outcome == Decision::Approved
        && gate.gate_type == KIND_INTEGRATION_GRANT
    {
        resume(services, &gate.workspace_id, person, &gate.fleet_id).await;
    }
}

async fn resume<D: Services>(
    services: &Arc<D>,
    workspace: &Uuid7,
    person: &PersonIdentity,
    fleet: &Uuid7,
) {
    let access = afd_authoring::Access {
        workspace,
        subject: person.subject(),
    };
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let fleet_id = fleet.as_str();
    match services
        .creations()
        .resume_setup(access, fleet, services.now())
        .await
    {
        Ok(saved) if saved.status == ACTIVE => {
            if after_activation(services.as_ref(), workspace, fleet, true)
                .await
                .is_err()
            {
                tracing::warn!(event = EVENT_SCHEDULES_UNSYNCED, fleet_id);
            }
        }
        Ok(_still_installing) => {}
        Err(failed) => {
            let error_code = failed.code().as_str();
            tracing::warn!(
                event = EVENT_SETUP_RESUME_FAILED,
                fleet_id,
                error_code,
                "an approved setup grant could not restart setup"
            );
        }
    }
}
