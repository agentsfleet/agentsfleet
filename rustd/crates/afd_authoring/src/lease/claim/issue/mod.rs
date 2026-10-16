//! The capability is revealed once; only its digest enters durable storage.

mod payload;

use afd_core::{clock::UnixMillis, error_code, id::Uuid7, timing::LEASE_TTL_MS};
use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::FailureReason;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;

use super::{
    Claimed, context,
    selection::{Hint, Selected},
    sql,
};
use crate::{
    Drafts, Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    funnel::Owed,
    lease::Status,
    model::{self, budget},
    types::{MessageState, QUEUED, RUNNING},
};

const CAPABILITY_BYTES: usize = 32;
pub(super) const WALL_SECONDS: i64 = 300;
const FENCE: i64 = 1;

pub(super) struct Placement {
    pub(super) attempt: Uuid7,
    pub(super) lease: Uuid7,
    pub(super) token: String,
    pub(super) now: UnixMillis,
    pub(super) until: i64,
    pub(super) deadline: i64,
    pub(super) reserved: i64,
}

pub(super) async fn admit(
    drafts: &Drafts,
    connection: &mut PgConnection,
    runner: &Uuid7,
    hint: &Hint,
    selected: Selected,
    owed: &mut Owed,
) -> Result<Option<Claimed>> {
    let prepared = prepare(connection, hint, &selected).await;
    let gate = Gate {
        hint,
        selected: &selected,
    };
    let Some((model, context, reserved)) = allowed(connection, &gate, prepared, owed).await? else {
        return Ok(None);
    };
    let placement = mint(drafts, connection, reserved).await?;
    let built = payload::build(hint, &selected, &model, context, &placement);
    let Some(payload) = allowed(connection, &gate, built, owed).await? else {
        return Ok(None);
    };
    insert(connection, runner, hint, &selected, &model, &placement).await?;
    sqlx::query(sql::RUNNING)
        .bind(hint.id.as_str())
        .bind(selected.sequence)
        .bind(RUNNING)
        .bind(placement.now.as_millis())
        .bind(QUEUED)
        .execute(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(Some(payload))
}

/// The message a refused gate fails, and whose draft it belongs to.
struct Gate<'a> {
    hint: &'a Hint,
    selected: &'a Selected,
}

async fn allowed<T>(
    connection: &mut PgConnection,
    gate: &Gate<'_>,
    result: Result<T>,
    owed: &mut Owed,
) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            let reason = match error.code() {
                error_code::AUTHORING_MODEL => FailureReason::ModelUnavailable,
                error_code::AUTHORING_BALANCE => FailureReason::InsufficientBalance,
                error_code::AUTHORING_TOKEN => FailureReason::ContextTooLarge,
                _ => return Err(error),
            };
            failure(connection, gate, reason, owed).await?;
            Ok(None)
        }
    }
}

async fn prepare(
    connection: &mut PgConnection,
    hint: &Hint,
    selected: &Selected,
) -> Result<(RegisteredModel, context::Context, i64)> {
    let model = model::selected(connection, &hint.tenant_id, &selected.model_ref).await?;
    let bound = budget::bound(&model)?;
    coverage(connection, hint, bound).await?;
    let context = context::read(connection, selected, &model).await?;
    Ok((model, context, bound))
}

async fn coverage(connection: &mut PgConnection, hint: &Hint, required: i64) -> Result<()> {
    let totals: Option<(i64, i64, i64)> = sqlx::query_as(sql::COVERAGE)
        .bind(hint.tenant_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let (balance, count, maximum) = totals.ok_or(ErrorKind::Balance)?;
    let obligation = count
        .checked_add(1)
        .and_then(|total| total.checked_mul(maximum.max(required)))
        .ok_or(ErrorKind::Balance)?;
    if balance < obligation {
        return Err(ErrorKind::Balance.into());
    }
    Ok(())
}

async fn mint(drafts: &Drafts, connection: &mut PgConnection, reserved: i64) -> Result<Placement> {
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let now = UnixMillis::from_millis(now);
    let mut bytes = [0u8; CAPABILITY_BYTES];
    drafts.entropy.fill(&mut bytes)?;
    Ok(Placement {
        attempt: drafts.mint(now)?,
        lease: drafts.mint(now)?,
        token: URL_SAFE_NO_PAD.encode(bytes),
        until: now.saturating_add_millis(LEASE_TTL_MS).as_millis(),
        deadline: now
            .saturating_add_millis(WALL_SECONDS * afd_billing::nanos::MS_PER_SEC)
            .as_millis(),
        now,
        reserved,
    })
}

async fn insert(
    connection: &mut PgConnection,
    runner: &Uuid7,
    hint: &Hint,
    selected: &Selected,
    model: &RegisteredModel,
    placement: &Placement,
) -> Result<()> {
    sqlx::query(sql::INSERT)
        .bind(placement.attempt.as_str())
        .bind(placement.lease.as_str())
        .bind(runner.as_str())
        .bind(hint.workspace_id.as_str())
        .bind(hint.tenant_id.as_str())
        .bind(hint.creator_id.as_str())
        .bind(hint.id.as_str())
        .bind(selected.sequence)
        .bind(FENCE)
        .bind(selected.draft.deletion_generation)
        .bind(hex::encode(Sha256::digest(placement.token.as_bytes())))
        .bind(serde_json::to_string(model).map_err(stored)?)
        .bind(Status::Active.as_str())
        .bind(placement.until)
        .bind(placement.deadline)
        .bind(placement.reserved)
        .bind(placement.now.as_millis())
        .execute(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(())
}

async fn failure(
    connection: &mut PgConnection,
    gate: &Gate<'_>,
    reason: FailureReason,
    owed: &mut Owed,
) -> Result<()> {
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let failed = MessageState::Failed.as_str();
    let changed = sqlx::query(sql::FAIL)
        .bind(gate.selected.draft.id.as_str())
        .bind(gate.selected.sequence)
        .bind(failed)
        .bind(crate::lease::report::verdict::reason(reason))
        .bind(now)
        .bind(QUEUED)
        .execute(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    if changed.rows_affected() == 1 {
        owed.finished(connection, gate.hint.into(), failed).await?;
    }
    Ok(())
}
