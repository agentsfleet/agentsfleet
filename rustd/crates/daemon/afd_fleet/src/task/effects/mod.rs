//! Immutable expected-head publication through the existing task/lease owner.

mod allocate;
mod maintenance;
mod read;
mod record;
mod recover;
mod row;

use super::{TaskLease, TaskRefusal, Tasks};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::task::{TaskEffect, TaskEffectInput, TaskEffectsPage, TaskEffectsQuery};

pub(in crate::task) use record::PublicationPermit;

pub(super) const PREPARATION_SECONDS: f64 = 90.0;

pub(super) async fn maintain(tasks: &Tasks) -> Result<super::EffectMaintenance> {
    maintenance::sweep(tasks).await
}

pub(super) async fn publish(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<TaskEffectInput>,
) -> Result<(TaskEffect, bool)> {
    let (effect, replayed) = allocate::reserve(tasks, lease, input).await?;
    if replayed {
        return Ok((read::one(tasks, lease, &effect.id).await?, true));
    }
    let github = tasks
        .dependencies
        .github
        .as_ref()
        .ok_or(TaskRefusal::InvalidTarget)?;
    let session = match github.session(lease).await {
        Ok(session) => session,
        Err(error) => {
            record::refuse(tasks, &effect).await?;
            return Err(error);
        }
    };
    if let Err(error) = session.prepare_tree(effect.target()?, input).await {
        record::refuse(tasks, &effect).await?;
        return Err(error);
    }
    let recovery = match github
        .seal_recovery(lease, &effect.workspace_id, &effect.id, effect.target()?)
        .await
    {
        Ok(recovery) => recovery,
        Err(error) => {
            record::refuse(tasks, &effect).await?;
            return Err(error);
        }
    };
    // This transaction is the committed stop cutoff. No ref request exists
    // before it; a stop after issuance retains an explicit in-flight effect.
    let permit = match record::issue(tasks, lease, input, &effect, &recovery).await {
        Ok(permit) => permit,
        Err(error) => {
            record::refuse(tasks, &effect).await?;
            return Err(error);
        }
    };
    match session
        .with_deadline(permit.deadline())
        .publish(permit)
        .await
    {
        Ok(commit) => record::confirm(tasks, &effect, &commit).await?,
        Err(error) => {
            record::unknown(tasks, &effect).await?;
            tracing::warn!(event = "dt_effect_uncertain", effect_id = %effect.id, error_code = %error.code());
        }
    }
    Ok((read::one(tasks, lease, &effect.id).await?, false))
}

pub(super) async fn one(tasks: &Tasks, lease: TaskLease<'_>, id: &Uuid7) -> Result<TaskEffect> {
    read::one(tasks, lease, id).await
}

pub(super) async fn list(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<TaskEffectsQuery>,
) -> Result<TaskEffectsPage> {
    read::list(tasks, lease, input).await
}
