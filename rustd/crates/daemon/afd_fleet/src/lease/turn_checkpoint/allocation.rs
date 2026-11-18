//! Head locking serializes retention and every monotonic generation allocation.

use af_core::id::Uuid7;
use af_wire::lease::{TurnCheckpointBegin, TurnWorkspace};

use super::{
    TurnCheckpointRefusal,
    guard::{self, TurnLeaseGuard},
};
use crate::{Result, error::query, lease::Leases};

pub(super) async fn head(owner: &mut TurnLeaseGuard<'_>) -> Result<Option<Uuid7>> {
    sqlx::query(include_str!("sql/head_insert.sql"))
        .bind(&owner.current.event_id)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("initialize event-owned recovery head"))?;
    let (_event, head, _next): (Uuid7, Option<Uuid7>, i64) =
        sqlx::query_as(include_str!("sql/head.sql"))
            .bind(&owner.current.event_id)
            .fetch_one(&mut *owner.tx)
            .await
            .map_err(query("hold event-owned recovery head"))?;
    owner.fresh().await?;
    Ok(head)
}

pub(super) async fn room(owner: &mut TurnLeaseGuard<'_>, head: Option<&Uuid7>) -> Result<()> {
    sqlx::query(include_str!("sql/abandon_stale.sql"))
        .bind(&owner.current.event_id)
        .bind(&owner.current.id)
        .bind(owner.current.fence)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("abandon only obsolete turn capture owners"))?;
    let pending: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/pending.sql"))
        .bind(&owner.current.event_id)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("prove one pending turn capture"))?;
    if pending.is_some() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let ready: i64 = sqlx::query_scalar(include_str!("sql/ready_count.sql"))
        .bind(&owner.current.event_id)
        .fetch_one(&mut *owner.tx)
        .await
        .map_err(query("count retained ready turn generations"))?;
    if ready >= super::READY_GENERATIONS {
        let retired: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/retire_ready.sql"))
            .bind(&owner.current.event_id)
            .bind(head)
            .fetch_optional(&mut *owner.tx)
            .await
            .map_err(query("retire an unpinned non-head turn generation"))?;
        if retired.is_none() || ready > super::READY_GENERATIONS {
            return Err(TurnCheckpointRefusal::Limit.into());
        }
    }
    owner.fresh().await?;
    Ok(())
}

pub(super) async fn create(
    leases: &Leases,
    owner: &mut TurnLeaseGuard<'_>,
    input: &TurnCheckpointBegin,
    operation: &Uuid7,
    digest: &str,
) -> Result<Uuid7> {
    let expected = input.expected_head.as_ref().map(guard::id).transpose()?;
    let (_head, generation): (Option<Uuid7>, i64) =
        sqlx::query_as(include_str!("sql/head_allocate.sql"))
            .bind(&owner.current.event_id)
            .bind(&expected)
            .bind(i64::MAX)
            .fetch_optional(&mut *owner.tx)
            .await
            .map_err(query("allocate next turn generation"))?
            .ok_or(TurnCheckpointRefusal::Limit)?;
    let now = owner.fresh().await?;
    let id = leases.entropy().uuid7(now)?;
    let pages = std::iter::repeat_with(|| leases.entropy().uuid7(now))
        .take(usize::try_from(input.page_count).map_err(|_overflow| TurnCheckpointRefusal::Limit)?)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    insert(owner, input, &id, generation, expected.as_ref(), &pages).await?;
    sqlx::query(include_str!("sql/operation_insert.sql"))
        .bind(&id)
        .bind(&owner.current.event_id)
        .bind(&owner.current.fleet_id)
        .bind(&owner.current.workspace_id)
        .bind(operation)
        .bind(digest)
        .bind(generation)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("retain immutable turn allocation identity"))?;
    sqlx::query(include_str!("sql/pages_allocate.sql"))
        .bind(&id)
        .bind(&pages)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("allocate complete ordered turn manifest pages"))?;
    Ok(id)
}

async fn insert(
    owner: &mut TurnLeaseGuard<'_>,
    input: &TurnCheckpointBegin,
    id: &Uuid7,
    generation: i64,
    expected: Option<&Uuid7>,
    pages: &[Uuid7],
) -> Result<()> {
    let toolbox = match &input.workspace {
        TurnWorkspace::ApiOnly => None,
        TurnWorkspace::Files { toolbox_digest } => Some(toolbox_digest),
    };
    sqlx::query(include_str!("sql/plan_insert.sql"))
        .bind(id)
        .bind(&owner.current.event_id)
        .bind(&owner.current.fleet_id)
        .bind(&owner.current.workspace_id)
        .bind(&owner.setup)
        .bind(generation)
        .bind(expected)
        .bind(&owner.current.id)
        .bind(owner.current.fence)
        .bind(
            i32::try_from(input.format_version)
                .map_err(|_overflow| TurnCheckpointRefusal::Incompatible)?,
        )
        .bind(toolbox.is_some())
        .bind(toolbox)
        .bind(i32::try_from(input.page_count).map_err(|_overflow| TurnCheckpointRefusal::Limit)?)
        .bind(pages)
        .bind(
            i64::try_from(input.upload_budget_bytes)
                .map_err(|_overflow| TurnCheckpointRefusal::Limit)?,
        )
        .bind(i64::try_from(input.logical_bytes).map_err(|_overflow| TurnCheckpointRefusal::Limit)?)
        .bind(i32::try_from(input.entry_count).map_err(|_overflow| TurnCheckpointRefusal::Limit)?)
        .bind(&input.history_digest)
        .bind(super::TRANSFER_SECONDS)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("persist complete turn capture plan"))?;
    Ok(())
}
