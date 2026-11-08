//! Immutable reviewed definitions and the catalogue's current selection.

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::decision::{DecisionConfiguration, DecisionModelSettings};
use jiff::Timestamp;
use sqlx::{PgConnection, types::Json};

use crate::{
    Result,
    error::{query, row},
    sql,
};

#[derive(sqlx::FromRow)]
struct Remembered {
    id: String,
    identical: bool,
}

/// Selects a definition owned by this model in the caller's mutation transaction.
pub(crate) async fn select(
    connection: &mut PgConnection,
    entropy: &Entropy,
    model: &Uuid7,
    configuration: &DecisionConfiguration,
    now: Timestamp,
) -> Result<Uuid7> {
    let candidate = entropy.uuid7(now)?;
    let revision = remember(connection, model, &candidate, &configuration.profile).await?;
    sqlx::query(sql::SELECT_DECISION_REVISION)
        .bind(model.as_str())
        .bind(revision.as_str())
        .bind(configuration.enabled)
        .execute(connection)
        .await
        .map_err(query("select reviewed decision revision"))?;
    Ok(revision)
}

/// A historical revision cannot acquire different bytes after rollback.
async fn remember(
    connection: &mut PgConnection,
    model: &Uuid7,
    candidate: &Uuid7,
    profile: &DecisionModelSettings,
) -> Result<Uuid7> {
    sqlx::query(sql::INSERT_DECISION_REVISION)
        .bind(candidate.as_str())
        .bind(model.as_str())
        .bind(&profile.revision)
        .bind(Json(profile))
        .execute(&mut *connection)
        .await
        .map_err(query("remember reviewed decision revision"))?;
    let remembered: Remembered = sqlx::query_as(sql::MATCH_DECISION_REVISION)
        .bind(model.as_str())
        .bind(&profile.revision)
        .bind(Json(profile))
        .fetch_one(connection)
        .await
        .map_err(query("verify immutable decision revision"))?;
    if !remembered.identical {
        return Err(crate::error::decision_revision());
    }
    Uuid7::parse(&remembered.id).map_err(row("model.decision_revisions", "id"))
}
