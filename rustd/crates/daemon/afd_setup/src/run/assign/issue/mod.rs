//! The capability is revealed once; only its digest enters durable storage.

mod payload;

use af_core::{clock, error_code, id::Uuid7};
use af_wire::authoring::FailureReason;
use afd_credential::provider::RegisteredModel;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;

use super::{
    AssignedDraft, context,
    selection::{Hint, Selected},
    sql,
};
use crate::{
    Drafts, Result,
    error::raise::{query, stored},
    funnel::Owed,
    model::{self, budget},
    run::Status,
    types::{MessageState, QUEUED, RUNNING},
};

const CAPABILITY_BYTES: usize = 32;
const FENCE: i64 = 1;

pub(super) struct Placement {
    pub(super) attempt: Uuid7,
    pub(super) run: Uuid7,
    pub(super) token: String,
    pub(super) until: Timestamp,
    pub(super) deadline: Timestamp,
    pub(super) reserved: i64,
    pub(super) policy: crate::turn_policy::TurnPolicy,
}

pub(super) async fn assign(
    drafts: &Drafts,
    connection: &mut PgConnection,
    runner: &Uuid7,
    hint: &Hint,
    selected: Selected,
    owed: &mut Owed,
) -> Result<Option<AssignedDraft>> {
    let prepared = prepare(drafts, connection, hint, &selected).await;
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
        .bind(&hint.id)
        .bind(selected.sequence)
        .bind(RUNNING)
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
    drafts: &Drafts,
    connection: &mut PgConnection,
    hint: &Hint,
    selected: &Selected,
) -> Result<(RegisteredModel, context::Context, i64)> {
    let model = model::selected(connection, &hint.tenant_id, &selected.model_ref).await?;
    let bound = budget::bound(&model, drafts.turn_policy)?;
    budget::gate(connection, &hint.tenant_id, bound).await?;
    let context = context::read(connection, selected, &model).await?;
    Ok((model, context, bound))
}

async fn mint(drafts: &Drafts, connection: &mut PgConnection, reserved: i64) -> Result<Placement> {
    // The deadlines leave in the payload before the row is written, so they
    // are computed here from the database clock rather than in the insert.
    let now = crate::sql::clock(connection, sql::CONTEXT).await?;
    let mut bytes = [0u8; CAPABILITY_BYTES];
    drafts.entropy.fill(&mut bytes)?;
    Ok(Placement {
        attempt: drafts.mint(now)?,
        run: drafts.mint(now)?,
        token: URL_SAFE_NO_PAD.encode(bytes),
        until: clock::after(now, drafts.clock.ttl().min(drafts.turn_policy.wall())),
        deadline: clock::after(now, drafts.turn_policy.wall()),
        policy: drafts.turn_policy,
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
        .bind(&placement.attempt)
        .bind(&placement.run)
        .bind(runner)
        .bind(&hint.workspace_id)
        .bind(&hint.tenant_id)
        .bind(&hint.user_id)
        .bind(&hint.id)
        .bind(selected.sequence)
        .bind(FENCE)
        .bind(hex::encode(Sha256::digest(placement.token.as_bytes())))
        .bind(serde_json::to_string(model).map_err(stored)?)
        .bind(Status::Active.as_str())
        .bind(placement.until.to_sqlx())
        .bind(placement.deadline.to_sqlx())
        .bind(placement.reserved)
        .bind(i64::from(placement.policy.settings().max_model_calls))
        .bind(i64::from(placement.policy.settings().max_wall_seconds))
        .bind(i64::from(
            placement.policy.settings().max_proposal_corrections,
        ))
        .bind(i64::from(
            placement.policy.settings().provider_timeout_seconds,
        ))
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
    // The assignment holds the draft's lock, which every writer of a message takes,
    // so the message it selected queued is still queued: this write finishes it.
    let failed = MessageState::Failed.as_str();
    sqlx::query(crate::sql::message::FAIL)
        .bind(&gate.selected.draft.id)
        .bind(gate.selected.sequence)
        .bind(failed)
        .bind(crate::run::report::verdict::reason(reason))
        .bind(QUEUED)
        .execute(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    owed.finished(connection, gate.hint.into(), failed).await?;
    Ok(())
}
