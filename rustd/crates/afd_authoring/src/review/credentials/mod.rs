//! Credential, connection and standing-grant checks remain separate observations.

mod connection;
mod purpose;

use afd_credential::{secrets::Supply, vault::CredentialInspection};
use serde_json::json;
use sqlx::PgConnection;

use super::{Observed, readiness};
use crate::{
    Result, Setup,
    error::raise::query,
    types::{Action, ActionKind, CheckKind, CheckState, Requirements},
};

const CONTEXT: &str = "observe reviewed integration access";
const GRANT: &str =
    "SELECT status FROM core.integration_grants WHERE fleet_id=$1::uuid AND service=$2 FOR SHARE";

pub(super) async fn observe(
    setup: &Setup,
    connection: &mut PgConnection,
    draft: &super::Context,
    candidate: &crate::types::Candidate,
    requirements: &Requirements,
    observed: &mut Observed,
) -> Result<()> {
    let needs = purpose::collect(candidate, requirements);
    let names: Vec<_> = needs.keys().map(|name| (*name).to_owned()).collect();
    let held = setup
        .vault
        .inspect_on(connection, &draft.workspace_id, &names, &setup.connectors)
        .await?;
    for (name, purposes) in needs {
        let held = held.iter().find(|held| held.name == name);
        observed.check(
            CheckKind::Credential,
            name,
            match held {
                Some(held) if purpose::usable(&purposes, held) => CheckState::Ready,
                Some(_) => CheckState::Blocked,
                None => CheckState::Missing,
            },
            Some(readiness::connections(draft)),
        );
        observed.versions.push(json!({"kind":"credential", "name":name, "version":held.map(|held| &held.resolution_version)}));
        if let Some(held) = held.filter(|held| {
            purpose::usable(&purposes, held) && purpose::needs_integration(&purposes)
        }) {
            integration(connection, draft, held, observed).await?;
        }
    }
    Ok(())
}

async fn integration(
    connection: &mut PgConnection,
    draft: &super::Context,
    held: &CredentialInspection,
    observed: &mut Observed,
) -> Result<()> {
    let Some(service) = &held.integration else {
        return Ok(());
    };
    if held.supply == Supply::Inline {
        if service == afd_credential::secrets::connector::SLACK_INTEGRATION {
            connection::observe(connection, draft, held, observed).await?;
        }
        return Ok(());
    }
    connection::observe(connection, draft, held, observed).await?;
    if held.supply == Supply::OnDemand {
        grant(connection, draft, service, observed).await?;
    }
    Ok(())
}

async fn grant(
    connection: &mut PgConnection,
    draft: &super::Context,
    service: &str,
    observed: &mut Observed,
) -> Result<()> {
    let status: Option<String> = sqlx::query_scalar(GRANT)
        .bind(
            draft
                .linked_fleet_id
                .as_ref()
                .map(afd_core::id::Uuid7::as_str),
        )
        .bind(service)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    observed.check(
        CheckKind::Grant,
        service,
        match status.as_deref() {
            Some("approved") => CheckState::Ready,
            Some("revoked" | "denied") => CheckState::Blocked,
            _ => CheckState::Missing,
        },
        Some(Action {
            kind: ActionKind::Approvals,
            label: "Open workspace approvals".to_owned(),
            href: format!("/w/{}/approvals", draft.workspace_id),
        }),
    );
    Ok(())
}
