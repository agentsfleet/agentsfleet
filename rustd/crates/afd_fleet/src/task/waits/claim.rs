//! A worker-sized claim commits before any credential exchange or network read.

use super::{
    ReadPermit,
    row::{Candidate, Wait},
};
use crate::{
    Result,
    error::query,
    lease::{Leases, scope::MintScope},
    task::{TaskRefusal, Tasks, guard, transaction},
};
use afd_core::{clock, id::Uuid7};
use afd_wire::task::TaskState;
use sqlx::PgConnection;
use std::time::Duration;

pub(super) struct Claimed {
    pub owner: Wait,
    pub claim: Uuid7,
    pub permit: ReadPermit,
}

pub(super) async fn claim(tasks: &Tasks, candidate: &Candidate) -> Result<Option<Claimed>> {
    let scope = candidate.scope()?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    guard::scope(&mut tx, &scope).await?;
    let _task: Option<String> = sqlx::query_scalar(crate::task::sql::LOCK_TASK)
        .bind(&candidate.task_id)
        .bind(&candidate.fleet_id)
        .bind(&candidate.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock due wait task"))?;
    let Some(owner) = owner(&mut tx, &candidate.id).await? else {
        return Ok(None);
    };
    if !owner.current() {
        super::cancel(&mut tx, &owner.task_id).await?;
        tx.commit()
            .await
            .map_err(query("cancel superseded native wait"))?;
        return Ok(None);
    }
    if !owner.within_limits {
        stop(
            &mut tx,
            &owner,
            TaskState::Exhausted,
            "cumulative_limit_reached",
        )
        .await?;
        tx.commit()
            .await
            .map_err(query("stop exhausted wait without starting an agent"))?;
        return Ok(None);
    }
    let fallback = owner.fallback();
    if !owner.due
        || owner.claim_valid == Some(true)
        || fallback && owner.used_fallback_reads >= owner.max_fallback_reads
    {
        return Ok(None);
    }
    let binding = match authority(&mut tx, &owner).await {
        Ok(binding) => binding,
        Err(fault) if fault.is_datastore_unavailable() => return Err(fault),
        Err(fault) => {
            stop(
                &mut tx,
                &owner,
                TaskState::Blocked,
                "dependency_access_unavailable",
            )
            .await?;
            tx.commit()
                .await
                .map_err(query("stop wait whose native authority was revoked"))?;
            tracing::warn!(event = "dt_wait_authority_refused", task_id = owner.task_id.as_str(), error_code = %fault.code());
            return Ok(None);
        }
    };
    let claim = tasks.leases.entropy().uuid7(clock::now())?;
    if fallback {
        sqlx::query(include_str!("sql/charge.sql"))
            .bind(&owner.task_id)
            .bind("dependency_read_limit")
            .execute(&mut *tx)
            .await
            .map_err(query("charge one bounded dependency observation"))?;
    }
    let until: jiff_sqlx::Timestamp = sqlx::query_scalar(include_str!("sql/claim.sql"))
        .bind(&owner.id)
        .bind(claim.as_str())
        .bind(CLAIM_TTL)
        .bind(i32::from(fallback))
        .fetch_one(&mut *tx)
        .await
        .map_err(query("reserve bounded native wait read"))?;
    tx.commit()
        .await
        .map_err(query("commit wait read intent before network access"))?;
    transaction::recycle(connection);
    let permit = ReadPermit {
        scope: MintScope {
            workspace_id: scope.workspace_id,
            fleet_id: scope.fleet_id,
            event_id: owner.id.clone().into_boxed_str(),
            binding: Some(binding),
        },
        installation: owner.target.installation_id,
        deadline: until.to_jiff().min(owner.expires_at),
    };
    Ok(Some(Claimed {
        owner,
        claim,
        permit,
    }))
}

pub(super) async fn owner(connection: &mut PgConnection, id: &str) -> Result<Option<Wait>> {
    sqlx::query_as(include_str!("sql/owner.sql"))
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(query("hold current wait after its task lock"))
}

pub(super) async fn authority(
    connection: &mut PgConnection,
    owner: &Wait,
) -> Result<afd_fleet_runtime::config::RepositoryBinding> {
    let mut live = None;
    for revision in [Some(owner.accepted_revision_id.as_str()), None] {
        let config = Leases::stored_config_on(connection, &owner.fleet_id, revision)
            .await?
            .ok_or(TaskRefusal::InvalidTarget)?;
        owner.target.within(&config)?;
        if revision.is_none() {
            live = config
                .repository_binding()
                .and_then(|binding| binding.read_repository(&owner.target.repository));
        }
    }
    let scope = super::super::TaskScope {
        fleet_id: guard::id(&owner.fleet_id, "fleet_id")?,
        workspace_id: guard::id(&owner.workspace_id, "workspace_id")?,
    };
    super::super::Github::hold_authority(connection, &scope, owner.target.installation_id).await?;
    live.ok_or_else(|| TaskRefusal::InvalidTarget.into())
}

pub(super) async fn stop(
    connection: &mut PgConnection,
    owner: &Wait,
    state: TaskState,
    reason: &str,
) -> Result<()> {
    super::cancel(connection, &owner.task_id).await?;
    sqlx::query(include_str!("sql/transition.sql"))
        .bind(&owner.task_id)
        .bind(state.as_str())
        .bind(reason)
        .bind(state.is_terminal())
        .execute(&mut *connection)
        .await
        .map_err(query("commit native wait blocker without an agent"))?;
    crate::task::continuation::retire(connection, &owner.task_id).await?;
    tracing::info!(
        event = "dt_transition",
        task_id = owner.task_id.as_str(),
        state = state.as_str(),
        reason
    );
    Ok(())
}

pub(super) const CLAIM_TTL: Duration = Duration::from_secs(30);
