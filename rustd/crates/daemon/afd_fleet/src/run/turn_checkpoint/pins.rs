//! A pin grants bounded custody only after live connection and run proof.

use af_core::id::Uuid7;
use af_wire::run::{
    TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointRef, TurnCheckpointResponse,
};
use af_wire::task::CheckpointPin;
use sqlx::PgConnection;

use super::{
    TurnCheckpointRefusal, TurnRun, allocation,
    guard::{self, TurnRunGuard},
    row::Plan,
    transaction,
};
use crate::{Result, error::query, run::Runs};

#[derive(sqlx::FromRow)]
pub(super) struct Pin {
    id: Uuid7,
    owner_run_id: Uuid7,
    owner_fence: i64,
    kind: String,
    operation_id: Uuid7,
    #[sqlx(try_from = "i64")]
    resource_version: u64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    operation_expires_at: jiff::Timestamp,
    released: bool,
}

impl Pin {
    pub(super) fn wire(&self) -> CheckpointPin {
        CheckpointPin {
            id: self.id.as_str().to_owned(),
            resource_version: self.resource_version,
            operation_deadline: self.operation_expires_at,
            expires_at: self.expires_at,
        }
    }

    fn proves(&self, owner: &TurnRunGuard<'_>, now: jiff::Timestamp) -> bool {
        self.belongs(owner, super::WRITER) && self.live(now)
    }

    pub(super) fn belongs(&self, owner: &TurnRunGuard<'_>, kind: &str) -> bool {
        self.owner_run_id == owner.current.id
            && self.owner_fence == owner.current.fence
            && self.kind == kind
    }

    pub(super) fn live(&self, now: jiff::Timestamp) -> bool {
        !self.released
            && self.expires_at > now
            && self.operation_expires_at > now
            && self.expires_at <= self.operation_expires_at
    }
}

pub(super) async fn writer(owner: &mut TurnRunGuard<'_>, checkpoint: &Uuid7) -> Result<Pin> {
    let pin: Pin = sqlx::query_as(include_str!("sql/writer_pin.sql"))
        .bind(checkpoint)
        .bind(super::WRITER)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("lock original turn writer pin"))?
        .ok_or(TurnCheckpointRefusal::Conflict)?;
    let now = owner.fresh().await?;
    if !pin.proves(owner, now) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(pin)
}

pub(super) async fn insert(
    runs: &Runs,
    owner: &mut TurnRunGuard<'_>,
    checkpoint: &Uuid7,
    deadline: jiff::Timestamp,
    operation: &Uuid7,
) -> Result<Pin> {
    let id = runs.entropy().uuid7(owner.fresh().await?)?;
    insert_on(
        &mut owner.tx,
        &id,
        (owner.current.id.clone(), owner.current.fence),
        checkpoint,
        deadline,
        super::WRITER,
        operation,
    )
    .await
}

async fn insert_on(
    connection: &mut PgConnection,
    id: &Uuid7,
    custodian: (Uuid7, i64),
    checkpoint: &Uuid7,
    deadline: jiff::Timestamp,
    kind: &str,
    operation: &Uuid7,
) -> Result<Pin> {
    sqlx::query_as(include_str!("sql/pin_insert.sql"))
        .bind(id)
        .bind(checkpoint)
        .bind(kind)
        .bind(custodian.0)
        .bind(custodian.1)
        .bind(jiff_sqlx::Timestamp::from(deadline))
        .bind(super::PIN_SECONDS)
        .bind(operation)
        .fetch_one(connection)
        .await
        .map_err(query("allocate bounded turn writer pin"))
}

pub(super) async fn exact(
    owner: &mut TurnRunGuard<'_>,
    reference: &TurnCheckpointRef,
) -> Result<Pin> {
    sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(guard::id(&reference.pin_id)?)
        .bind(guard::id(&reference.checkpoint_id)?)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("lock exact turn checkpoint custody"))?
        .ok_or_else(|| TurnCheckpointRefusal::Conflict.into())
}

pub(super) async fn release(owner: &mut TurnRunGuard<'_>, pin: &Pin) -> Result<()> {
    let changed = sqlx::query(include_str!("sql/pin_release.sql"))
        .bind(&pin.id)
        .bind(&owner.current.id)
        .bind(owner.current.fence)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("release exact owned turn checkpoint pin"))?;
    if changed.rows_affected() != 1 {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(())
}

pub(super) async fn reader(
    runs: &Runs,
    owner: &mut TurnRunGuard<'_>,
    checkpoint: &Uuid7,
    operation: &Uuid7,
) -> Result<Pin> {
    let previous: Option<Pin> = sqlx::query_as(include_str!("sql/reader_pin.sql"))
        .bind(checkpoint)
        .bind(&owner.current.id)
        .bind(owner.current.fence)
        .bind(super::READER)
        .bind(operation)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("read original turn restore custody"))?;
    let now = owner.fresh().await?;
    if let Some(pin) = previous {
        if pin.operation_id != *operation || !pin.belongs(owner, super::READER) || !pin.live(now) {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        return Ok(pin);
    }
    let count: i64 = sqlx::query_scalar(include_str!("sql/reader_capacity.sql"))
        .bind(checkpoint)
        .bind(super::READER)
        .fetch_one(&mut *owner.tx)
        .await
        .map_err(query("bound original turn reader operations"))?;
    if count >= 64 {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    let id = runs.entropy().uuid7(owner.fresh().await?)?;
    let duration = std::time::Duration::from_secs(
        u64::try_from(super::TRANSFER_SECONDS).map_err(|_overflow| TurnCheckpointRefusal::Limit)?,
    );
    let deadline = af_core::clock::after(now, duration);
    insert_on(
        &mut owner.tx,
        &id,
        (owner.current.id.clone(), owner.current.fence),
        checkpoint,
        deadline,
        super::READER,
        operation,
    )
    .await
}

pub(super) async fn update(
    runs: &Runs,
    run: TurnRun<'_>,
    input: &garde::Valid<TurnCheckpointCommand>,
) -> Result<TurnCheckpointResponse> {
    let (TurnCheckpointAction::PinRenew {
        checkpoint: reference,
        ..
    }
    | TurnCheckpointAction::PinRelease {
        checkpoint: reference,
    }
    | TurnCheckpointAction::Abandon {
        checkpoint: reference,
    }) = &input.action
    else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
    allocation::head(&mut owner).await?;
    let plan = Plan::load(&mut owner, &guard::id(&reference.checkpoint_id)?).await?;
    let _scope = plan.scope(&owner)?;
    let pin = exact(&mut owner, reference).await?;
    if !pin.belongs(&owner, super::WRITER) && !pin.belongs(&owner, super::READER) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let response = update_owned(&mut owner, &plan, pin, input).await?;
    owner.commit().await?;
    transaction::recycle(connection);
    Ok(response)
}

async fn renew(owner: &mut TurnRunGuard<'_>, pin: Pin, expected: u64) -> Result<Pin> {
    if pin.resource_version != expected {
        return if expected.checked_add(1) == Some(pin.resource_version) {
            Ok(pin)
        } else {
            Err(TurnCheckpointRefusal::Conflict.into())
        };
    }
    let expected = i64::try_from(expected).map_err(|_overflow| TurnCheckpointRefusal::Limit)?;
    let now = owner.fresh().await?;
    sqlx::query_as(include_str!("sql/pin_renew.sql"))
        .bind(&pin.id)
        .bind(expected)
        .bind(i64::MAX)
        .bind(jiff_sqlx::Timestamp::from(now))
        .bind(super::PIN_SECONDS)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query(
            "renew turn pin within its original operation deadline",
        ))?
        .ok_or_else(|| TurnCheckpointRefusal::Limit.into())
}

async fn abandon(owner: &mut TurnRunGuard<'_>, plan: &Plan, pin: &Pin) -> Result<()> {
    if plan.ready
        || !pin.belongs(owner, super::WRITER)
        || plan.owner_run_id != owner.current.id
        || plan.owner_fence != owner.current.fence
    {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    sqlx::query(include_str!("sql/abandon.sql"))
        .bind(&plan.id)
        .execute(&mut *owner.tx)
        .await
        .map_err(query(
            "abandon pending turn capture without moving the head",
        ))?;
    release(owner, pin).await
}

async fn update_owned(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    pin: Pin,
    input: &garde::Valid<TurnCheckpointCommand>,
) -> Result<TurnCheckpointResponse> {
    match &input.action {
        TurnCheckpointAction::PinRenew {
            resource_version, ..
        } => {
            if plan.abandoned || !pin.live(owner.fresh().await?) {
                return Err(TurnCheckpointRefusal::Conflict.into());
            }
            Ok(TurnCheckpointResponse::PinRenewed(
                renew(owner, pin, *resource_version).await?.wire(),
            ))
        }
        TurnCheckpointAction::PinRelease { .. } => {
            release(owner, &pin).await?;
            Ok(TurnCheckpointResponse::PinReleased)
        }
        TurnCheckpointAction::Abandon { .. } => {
            abandon(owner, plan, &pin).await?;
            Ok(TurnCheckpointResponse::Abandoned)
        }
        _ => Err(TurnCheckpointRefusal::Incompatible.into()),
    }
}
