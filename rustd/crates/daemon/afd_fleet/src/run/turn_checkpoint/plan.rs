//! Stable operation receipts prevent replay from allocating another generation.

use af_core::id::Uuid7;
use af_wire::run::{
    TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand, TurnCheckpointPlan,
    TurnWorkspace,
};

use super::{
    Receipt, TurnCheckpointRefusal, TurnRun, allocation,
    guard::{self, TurnRunGuard},
    pins,
    row::Plan,
    transaction,
};
use crate::{Result, error::query, run::Runs};

impl Runs {
    /// Allocate one event-owned plan after proving the live ordinary attempt.
    /// The plan records `cipher`: its new objects and its root are sealed under it.
    ///
    /// # Errors
    /// Refuses foreign or expired authority, incompatible captures, changed
    /// replay content and exhausted retention. Datastore failures retain their cause.
    pub async fn begin_turn_checkpoint(
        &self,
        run: TurnRun<'_>,
        request: &garde::Valid<TurnCheckpointCommand>,
        cipher: af_wire::durable_task::CheckpointCipher,
    ) -> Result<TurnCheckpointPlan> {
        if request.fencing_token != run.fencing_token {
            return Err(crate::error::stale_fence());
        }
        let TurnCheckpointAction::Begin(begin) = &request.action else {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        };
        shape(begin)?;
        let digest = af_core::etag::sha256_hex(
            &serde_json::to_vec(&**request).map_err(af_core::json::Unreadable::from)?,
        );
        let operation = guard::id(&request.operation_id)?;
        let mut connection = transaction::acquire(self.pool()).await?;
        let mut owner = TurnRunGuard::lock(self, &mut connection, run).await?;
        let head = allocation::initialize_head(&mut owner).await?;
        let response = if let Some(plan) = replay(&mut owner, &operation, &digest).await? {
            plan
        } else {
            if head != begin.expected_head.as_ref().map(guard::id).transpose()? {
                return Err(TurnCheckpointRefusal::Conflict.into());
            }
            // A delta is applied over the head it was walked against, and
            // over nothing else.
            if begin.parent.is_some() && begin.parent != begin.expected_head {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
            allocation::room(&mut owner, head.as_ref()).await?;
            let receipt = (&operation, digest.as_str());
            let id = allocation::create(self, &mut owner, begin, receipt, cipher).await?;
            let plan = Plan::load(&mut owner, &id).await?;
            let pin = pins::insert(self, &mut owner, &id, plan.expires_at, &operation).await?;
            plan.wire(&owner, &pin, false)?
        };
        owner.commit().await?;
        transaction::recycle(connection);
        Ok(response)
    }
}

pub(super) fn shape(input: &TurnCheckpointBegin) -> Result<()> {
    let valid = match &input.workspace {
        TurnWorkspace::ApiOnly => {
            input.parent.is_none()
                && input.page_count == 0
                && input.logical_bytes == 0
                && input.entry_count == 0
                && input.upload_budget_bytes == 0
        }
        TurnWorkspace::Files { .. } => input.page_count > 0,
    };
    if !valid {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

async fn replay(
    owner: &mut TurnRunGuard<'_>,
    operation: &Uuid7,
    digest: &str,
) -> Result<Option<TurnCheckpointPlan>> {
    // The scope names the turn, and [`Plan::load`] proves the plan is the
    // turn's, so a receipt carries no owner columns.
    let receipt: Option<Receipt> = sqlx::query_as(super::RECEIPT_REPLAY)
        .bind(super::receipt_scope(
            &owner.current.workspace_id,
            &owner.current.turn_id,
        ))
        .bind(operation)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("read immutable turn allocation receipt"))?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_digest != digest {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let plan = Plan::load(owner, &receipt.resource_id).await?;
    if plan.expires_at <= owner.fresh().await? {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let pin = pins::writer(owner, &plan.id).await?;
    Ok(Some(plan.wire(owner, &pin, true)?))
}
