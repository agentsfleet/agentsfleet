//! Funding and key reads finish before native lifecycle locks are reacquired.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::task::DecisionReservationInput;
use afd_billing::{
    Accounts, Nanos, Posture,
    biller::{Account, Ask, Funds, WorkKey},
    rates::Estimate,
};
use sqlx::PgConnection;

use super::{
    ReservedDecision,
    dispatch::Issuance,
    replay,
    row::{self, Call, Lookup},
};
use crate::{
    Result,
    task::{TaskRefusal, TaskRun, Tasks, transaction},
};

pub(super) struct Coverage {
    pub headroom: Nanos,
    pub required: Nanos,
}

pub(super) async fn issue(
    tasks: &Tasks,
    task_run: TaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: Call,
) -> Result<ReservedDecision> {
    let result = run(tasks, task_run, input, &call).await;
    match result {
        Ok(response) => Ok(response),
        Err(fault) => {
            let saved = row::peek(tasks.runs.pool(), Lookup::Id(&call.id)).await?;
            if let Some(saved) = saved.filter(|saved| saved.issued || saved.accounted) {
                return replay::response(tasks, saved).await;
            }
            Err(fault)
        }
    }
}

async fn run(
    tasks: &Tasks,
    task_run: TaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: &Call,
) -> Result<ReservedDecision> {
    let remaining = u64::try_from(call.remaining_ms)
        .ok()
        .filter(|remaining| *remaining > 0)
        .ok_or(TaskRefusal::DecisionUnavailable)?;
    let work = async {
        let coverage = cover(tasks, task_run, call).await?;
        let admin = tasks
            .dependencies
            .admin_workspace
            .as_ref()
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        let key = key(tasks, admin, call.profile_snapshot.0.profile.api).await?;
        Issuance {
            tasks,
            run: task_run,
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

async fn cover(tasks: &Tasks, task_run: TaskRun<'_>, call: &Call) -> Result<Coverage> {
    let proposed = proposed(tasks, task_run).await?;
    let ask = Ask {
        work: WorkKey::FleetRun(task_run.run_id.clone()),
        payer: Account {
            tenant: call.tenant_id.clone(),
            workspace: call.workspace_id.clone(),
        },
        next: proposed.required,
    };
    match tasks.dependencies.biller.check(&ask).await? {
        Funds::Covered => Ok(proposed),
        Funds::Short { .. } => Err(refused(task_run, "insufficient_credit")),
        Funds::Unconfirmed => Err(refused(task_run, "credit_unconfirmed")),
    }
}

async fn proposed(tasks: &Tasks, task_run: TaskRun<'_>) -> Result<Coverage> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let result = proposed_on(&mut connection, task_run).await;
    transaction::recycle(connection);
    result
}

async fn proposed_on(connection: &mut PgConnection, task_run: TaskRun<'_>) -> Result<Coverage> {
    let mut tx = transaction::begin(connection).await?;
    let current = crate::run::Runs::load_for_renew_on(&mut tx, task_run.run_id, task_run.runner_id)
        .await?
        .filter(|task_run| task_run.status == crate::run::sql::RUN_STATUS_ACTIVE)
        .ok_or(TaskRefusal::StaleState)?;
    let posture = Posture::parse(&current.posture).ok_or(TaskRefusal::DecisionUnavailable)?;
    let estimate = Accounts::estimate_on(
        &mut tx,
        posture,
        &current.provider,
        &current.model,
        af_core::clock::now(),
    )
    .await?;
    let Estimate::Priced(headroom) = estimate else {
        return Err(refused(task_run, "coding_price_unavailable"));
    };
    let required = super::credit::required_on(&mut tx, task_run.run_id, headroom).await?;
    tx.commit().await.map_err(crate::error::query(
        "finish bounded decision credit prerequisites",
    ))?;
    Ok(Coverage { headroom, required })
}

fn refused(task_run: TaskRun<'_>, reason: &'static str) -> crate::Error {
    tracing::warn!(
        event = "dt_decision_funding_refused",
        run_id = task_run.run_id.as_str(),
        reason
    );
    TaskRefusal::DecisionUnavailable.into()
}

async fn key(
    tasks: &Tasks,
    admin: &af_core::id::Uuid7,
    api: af_wire::decision::DecisionApi,
) -> Result<SecretString> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
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
