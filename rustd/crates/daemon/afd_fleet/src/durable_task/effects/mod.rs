//! Immutable expected-head publication through the existing task/run owner.

mod allocate;
mod read;
mod record;
mod recover;
mod row;

use super::{DurableTaskRefusal, DurableTaskRun, DurableTasks};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::durable_task::{
    DurableTaskEffect, DurableTaskEffectInput, DurableTaskEffectsPage, DurableTaskEffectsQuery,
};

pub(in crate::durable_task) use record::PublicationPermit;

pub(super) const PREPARATION_SECONDS: f64 = 90.0;

pub(super) async fn publish(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskEffectInput>,
) -> Result<(DurableTaskEffect, bool)> {
    let (effect, replayed) = allocate::reserve(tasks, run, input).await?;
    if replayed {
        return Ok((read::one(tasks, run, &effect.id).await?, true));
    }
    let github = tasks
        .dependencies
        .github
        .as_ref()
        .ok_or(DurableTaskRefusal::InvalidTarget)?;
    let session = match github.session(run).await {
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
        .seal_recovery(run, &effect.workspace_id, &effect.id, effect.target()?)
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
    let permit = match record::issue(tasks, run, input, &effect, &recovery).await {
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
            let (effect_id, error_code) = (&effect.id, error.code());
            {
                let event = &"dt_effect_uncertain";
                tracing::warn!(event, %effect_id, %error_code);
            };
        }
    }
    Ok((read::one(tasks, run, &effect.id).await?, false))
}

pub(super) async fn one(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    id: &Uuid7,
) -> Result<DurableTaskEffect> {
    read::one(tasks, run, id).await
}

pub(super) async fn list(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskEffectsQuery>,
) -> Result<DurableTaskEffectsPage> {
    read::list(tasks, run, input).await
}
