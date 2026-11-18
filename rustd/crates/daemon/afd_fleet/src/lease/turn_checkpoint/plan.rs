//! Stable operation receipts prevent replay from allocating another generation.

use af_core::id::Uuid7;
use af_wire::lease::{
    TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand, TurnCheckpointPlan,
    TurnWorkspace,
};

use super::{
    TurnCheckpointRefusal, TurnLease, allocation,
    guard::{self, TurnLeaseGuard},
    pins,
    row::Plan,
    transaction,
};
use crate::{Result, error::query, lease::Leases};

impl Leases {
    /// Allocate one event-owned plan after proving the live ordinary attempt.
    ///
    /// # Errors
    /// Refuses foreign or expired authority, incompatible captures, changed
    /// replay content and exhausted retention. Datastore failures retain their cause.
    pub async fn begin_turn_checkpoint(
        &self,
        lease: TurnLease<'_>,
        request: &garde::Valid<TurnCheckpointCommand>,
    ) -> Result<TurnCheckpointPlan> {
        if request.fencing_token != lease.fencing_token {
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
        let mut owner = TurnLeaseGuard::lock(self, &mut connection, lease).await?;
        let head = allocation::initialize_head(&mut owner).await?;
        let response = if let Some(plan) = replay(&mut owner, &operation, &digest).await? {
            plan
        } else {
            if head != begin.expected_head.as_ref().map(guard::id).transpose()? {
                return Err(TurnCheckpointRefusal::Conflict.into());
            }
            allocation::room(&mut owner, head.as_ref()).await?;
            let id = allocation::create(self, &mut owner, begin, &operation, &digest).await?;
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
            input.page_count == 0
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

#[derive(sqlx::FromRow)]
struct Receipt {
    checkpoint_id: Uuid7,
    fleet_id: Uuid7,
    workspace_id: Uuid7,
    request_digest: String,
    #[sqlx(try_from = "i64")]
    generation: u64,
}

async fn replay(
    owner: &mut TurnLeaseGuard<'_>,
    operation: &Uuid7,
    digest: &str,
) -> Result<Option<TurnCheckpointPlan>> {
    let receipt: Option<Receipt> = sqlx::query_as(include_str!("sql/operation_replay.sql"))
        .bind(&owner.current.event_id)
        .bind(operation)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("read immutable turn allocation receipt"))?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.fleet_id != owner.current.fleet_id
        || receipt.workspace_id != owner.current.workspace_id
        || receipt.request_digest != digest
    {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let plan = Plan::load(owner, &receipt.checkpoint_id).await?;
    if plan.generation != receipt.generation || plan.expires_at <= owner.fresh().await? {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let pin = pins::writer(owner, &plan.id).await?;
    Ok(Some(plan.wire(owner, &pin, true)?))
}
