//! Exact descriptors and live reader pins precede prefix adoption under the replacement fence.

use af_wire::run::{
    TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointRead, TurnCheckpointResponse,
};

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun, allocation, folds,
    guard::{self, TurnRunGuard},
    pins, root,
    row::Plan,
    transaction,
};
use crate::{Result, run::Runs};

pub(super) async fn read(
    store: &TurnCheckpoints,
    runs: &Runs,
    run: TurnRun<'_>,
    input: &garde::Valid<TurnCheckpointCommand>,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::Read { descriptor } = &input.action else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
    let head = allocation::head(&mut owner).await?;
    let id = guard::id(&descriptor.checkpoint_id)?;
    let plan = Plan::load(&mut owner, &id).await?;
    if head.as_ref() != Some(&id) || plan.descriptor(&owner)? != **descriptor {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let bytes = root::open(store, &mut owner, &plan).await?;
    let covered = plan.folds()?;
    root::validate(&bytes, &plan.scope(&owner)?, &covered, None)?;
    folds::validate(&mut owner, &covered, None).await?;
    let pin = pins::reader(runs, &mut owner, &plan.id, &guard::id(&input.operation_id)?).await?;
    let response = TurnCheckpointResponse::Read(Box::new(TurnCheckpointRead {
        descriptor: (**descriptor).clone(),
        pin: pin.wire(),
        page_ids: plan
            .page_ids
            .iter()
            .map(|id| af_wire::run::CheckpointOperationId(id.as_str().to_owned()))
            .collect(),
        root: bytes,
        folds: covered,
    }));
    owner.commit().await?;
    transaction::recycle(connection);
    Ok(response)
}

pub(super) async fn adopt(
    runs: &Runs,
    run: TurnRun<'_>,
    input: &garde::Valid<TurnCheckpointCommand>,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::Adopt { checkpoint } = &input.action else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
    let head = allocation::head(&mut owner).await?;
    let plan = Plan::load(&mut owner, &guard::id(&checkpoint.checkpoint_id)?).await?;
    if head.as_ref() != Some(&plan.id) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let _descriptor = plan.descriptor(&owner)?;
    let pin = pins::exact(&mut owner, checkpoint).await?;
    if !pin.belongs(&owner, super::READER) || !pin.live(owner.fresh().await?) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let covered = folds::adopt(&mut owner, &plan).await?;
    if !pin.live(owner.fresh().await?) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    owner.commit().await?;
    transaction::recycle(connection);
    Ok(TurnCheckpointResponse::Adopted(covered))
}
