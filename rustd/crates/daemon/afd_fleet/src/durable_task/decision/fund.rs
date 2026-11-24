//! The reservation's money reads and its key read: none takes a row lock, and the
//! wallet and the vault each answer on a connection of their own.

use af_crypto::secret::SecretString;
use afd_billing::{
    Accounts, Nanos, Posture,
    biller::{Account, Ask, Funds, WorkKey},
    rates::Estimate,
};
use sqlx::PgConnection;

use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTasks, guard::RunRow, transaction},
};

pub(super) struct Coverage {
    pub required: Nanos,
}

/// What the run must still be able to pay: its unaccounted decision reservations,
/// this call's, and the coding headroom its model's price book sets.
pub(super) async fn proposed(
    connection: &mut PgConnection,
    durable_task_run: DurableTaskRun<'_>,
) -> Result<Coverage> {
    let current = crate::run::Runs::load_for_renew_on(
        connection,
        durable_task_run.run_id,
        durable_task_run.runner_id,
    )
    .await?
    .filter(|run| run.status == crate::run::sql::RUN_STATUS_ACTIVE)
    .ok_or(DurableTaskRefusal::StaleState)?;
    let posture =
        Posture::parse(&current.posture).ok_or(DurableTaskRefusal::DecisionUnavailable)?;
    let estimate = Accounts::estimate_on(
        connection,
        posture,
        &current.provider,
        &current.model,
        af_core::clock::now(),
    )
    .await?;
    let Estimate::Priced(headroom) = estimate else {
        return Err(refused(durable_task_run, "coding_price_unavailable"));
    };
    let unaccounted =
        super::credit::required_on(connection, durable_task_run.run_id, headroom).await?;
    let required = unaccounted.saturating_add(Nanos::from_i64(super::budget::CALL_NANOS));
    Ok(Coverage { required })
}

/// The payer's wallet covers [`Coverage::required`].
pub(super) async fn cover(
    tasks: &DurableTasks,
    durable_task_run: DurableTaskRun<'_>,
    current: &RunRow,
    coverage: &Coverage,
) -> Result<()> {
    let ask = Ask {
        work: WorkKey::FleetRun(durable_task_run.run_id.clone()),
        payer: Account {
            tenant: current.tenant_id.clone(),
            workspace: current.workspace_id.clone(),
        },
        next: coverage.required,
    };
    match tasks.dependencies.biller.check(&ask).await? {
        Funds::Covered => Ok(()),
        Funds::Short { .. } => Err(refused(durable_task_run, "insufficient_credit")),
        Funds::Unconfirmed => Err(refused(durable_task_run, "credit_unconfirmed")),
    }
}

fn refused(durable_task_run: DurableTaskRun<'_>, reason: &'static str) -> crate::Error {
    let run_id = durable_task_run.run_id.as_str();
    {
        let event = &"dt_decision_funding_refused";
        tracing::warn!(event, run_id, reason);
    };
    DurableTaskRefusal::DecisionUnavailable.into()
}

/// The platform's provider key for `api`, from the vault.
pub(super) async fn key(
    tasks: &DurableTasks,
    api: af_wire::decision::DecisionApi,
) -> Result<SecretString> {
    let admin = tasks
        .dependencies
        .admin_workspace
        .as_ref()
        .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
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
