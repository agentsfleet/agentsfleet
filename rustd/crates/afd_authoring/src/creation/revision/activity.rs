//! The saved revision and its model-change notice commit together.

use super::super::{CONTEXT, stored::Stored};
use crate::{Result, error::raise::query};
use afd_core::id::Uuid7;
use serde_json::json;
use sqlx::PgConnection;

const ACTOR: &str = "config_reload";
const MODEL_CHANGED: &str = "model_changed";

pub(super) async fn record(
    connection: &mut PgConnection,
    row: &Stored,
    model: Option<&str>,
    revision: &Uuid7,
    number: i64,
) -> Result<()> {
    if row.runtime_model_ref.as_deref() == model {
        return Ok(());
    }
    let payload = json!({
        "previous_model_ref": row.runtime_model_ref,
        "model_ref": model,
        "setup_revision": number.to_string(),
    });
    sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
        .bind(row.id.as_str())
        .bind(revision.as_str())
        .bind(row.workspace_id.as_str())
        .bind(ACTOR)
        .bind(MODEL_CHANGED)
        .bind(sqlx::types::Json(payload))
        .bind(None::<&str>)
        .bind(afd_core::event::status::PROCESSED)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
