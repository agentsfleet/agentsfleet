//! Funding and key reads finish before native lifecycle locks are reacquired.

use std::time::Duration;

use afd_billing::{
    Accounts, Nanos, Posture,
    biller::{Account, Ask, Funds, WorkKey},
    rates::Estimate,
};
use afd_crypto::secret::SecretString;
use afd_wire::task::DecisionReservationInput;
use sqlx::PgConnection;

use super::{
    ReservedDecision,
    dispatch::Issuance,
    replay,
    row::{self, Call, Lookup},
};
use crate::{
    Result,
    task::{TaskLease, TaskRefusal, Tasks, transaction},
};

pub(super) struct Coverage {
    pub headroom: Nanos,
    pub required: Nanos,
}

pub(super) async fn issue(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: Call,
) -> Result<ReservedDecision> {
    let result = run(tasks, lease, input, &call).await;
    match result {
        Ok(response) => Ok(response),
        Err(fault) => {
            let saved = row::peek(tasks.leases.pool(), Lookup::Id(&call.id)).await?;
            if let Some(saved) = saved.filter(|saved| saved.issued || saved.accounted) {
                return replay::response(tasks, saved).await;
            }
            Err(fault)
        }
    }
}

async fn run(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: &Call,
) -> Result<ReservedDecision> {
    let remaining = u64::try_from(call.remaining_ms)
        .ok()
        .filter(|remaining| *remaining > 0)
        .ok_or(TaskRefusal::DecisionUnavailable)?;
    let work = async {
        let coverage = cover(tasks, lease, call).await?;
        let admin = tasks
            .dependencies
            .admin_workspace
            .as_ref()
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        let key = key(tasks, admin, call.profile_snapshot.0.profile.api).await?;
        Issuance {
            tasks,
            lease,
            input,
            call,
            coverage,
            key: &key,
        }
        .commit()
        .await
    };
    tokio::time::timeout(Duration::from_millis(remaining), work)
        .await
        .map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::DecisionDeadline { source })
        })?
}

async fn cover(tasks: &Tasks, lease: TaskLease<'_>, call: &Call) -> Result<Coverage> {
    let proposed = proposed(tasks, lease).await?;
    let ask = Ask {
        work: WorkKey::FleetLease(lease.lease_id.clone()),
        payer: Account {
            tenant: Call::id(&call.tenant_id, "tenant_id")?,
            workspace: Call::id(&call.workspace_id, "workspace_id")?,
        },
        next: proposed.required,
    };
    match tasks.dependencies.biller.check(&ask).await? {
        Funds::Covered => Ok(proposed),
        Funds::Short { .. } => Err(refused(lease, "insufficient_credit")),
        Funds::Unconfirmed => Err(refused(lease, "credit_unconfirmed")),
    }
}

async fn proposed(tasks: &Tasks, lease: TaskLease<'_>) -> Result<Coverage> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let result = proposed_on(&mut connection, lease).await;
    transaction::recycle(connection);
    result
}

async fn proposed_on(connection: &mut PgConnection, lease: TaskLease<'_>) -> Result<Coverage> {
    let mut tx = transaction::begin(connection).await?;
    let current =
        crate::lease::Leases::load_for_renew_on(&mut tx, lease.lease_id.as_str(), lease.runner_id)
            .await?
            .filter(|lease| lease.status == crate::lease::sql::LEASE_STATUS_ACTIVE)
            .ok_or(TaskRefusal::StaleState)?;
    let posture = Posture::parse(&current.posture).ok_or(TaskRefusal::DecisionUnavailable)?;
    let estimate = Accounts::estimate_on(
        &mut tx,
        posture,
        &current.provider,
        &current.model,
        afd_core::clock::now(),
    )
    .await?;
    let Estimate::Priced(headroom) = estimate else {
        return Err(refused(lease, "coding_price_unavailable"));
    };
    let required = super::credit::required_on(&mut tx, lease.lease_id, headroom).await?;
    tx.commit().await.map_err(crate::error::query(
        "finish bounded decision credit prerequisites",
    ))?;
    Ok(Coverage { headroom, required })
}

fn refused(lease: TaskLease<'_>, reason: &'static str) -> crate::Error {
    tracing::warn!(
        event = "dt_decision_funding_refused",
        lease_id = lease.lease_id.as_str(),
        reason
    );
    TaskRefusal::DecisionUnavailable.into()
}

async fn key(
    tasks: &Tasks,
    admin: &afd_core::id::Uuid7,
    api: afd_wire::decision::DecisionApi,
) -> Result<SecretString> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let result = async {
        let mut tx = transaction::begin(&mut connection).await?;
        let key = tasks
            .dependencies
            .providers
            .resolve_decision_key_on(&mut tx, admin, api)
            .await?;
        tx.commit().await.map_err(crate::error::query(
            "finish bounded platform-admin key lookup",
        ))?;
        Ok(key)
    }
    .await;
    transaction::recycle(connection);
    result
}
