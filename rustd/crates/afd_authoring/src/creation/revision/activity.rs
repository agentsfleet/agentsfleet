//! The saved revision and its model-change notice commit together.

use super::super::{CONTEXT, stored::Stored};
use crate::{
    Result,
    error::raise::{query, stored},
};
use afd_core::id::Uuid7;
use afd_dragonfly::FleetStreams;
use serde::Serialize;
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
) -> Result<bool> {
    if row.runtime_model_ref.as_deref() == model {
        return Ok(false);
    }
    let payload = serde_json::to_string(&json!({
        "previous_model_ref": row.runtime_model_ref,
        "model_ref": model,
        "setup_revision": number.to_string(),
    }))
    .map_err(stored)?;
    sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
        .bind(row.id.as_str())
        .bind(revision.as_str())
        .bind(row.workspace_id.as_str())
        .bind(ACTOR)
        .bind(MODEL_CHANGED)
        .bind(payload)
        .bind(None::<&str>)
        .bind(afd_core::event::status::PROCESSED)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(true)
}

/// Existing open tails recover the committed notice through ordinary history.
pub(super) async fn publish(streams: &FleetStreams, fleet: &Uuid7) {
    streams
        .publish_frame(fleet.as_str(), &Notice::CatchingUp { dropped: 0 })
        .await;
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Notice {
    CatchingUp { dropped: u64 },
}
