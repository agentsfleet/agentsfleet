//! Review binds the creator's actual account membership rather than a caller-provided role.

use super::{Context, Observed};
use crate::{Result, error::raise::query};
use serde_json::json;
use sqlx::PgConnection;

const CONTEXT: &str = "observe reviewed membership";
/// The authority version of daemon-written setup, which no membership change can move.
const SYSTEM_AUTHORITY: &str = "system";

pub(super) async fn observe(
    connection: &mut PgConnection,
    context: &Context,
    observed: &mut Observed,
) -> Result<()> {
    let version: Option<String> = match &context.creator_id {
        Some(creator) => sqlx::query_scalar(super::sql::AUTHORITY)
            .bind(creator.as_str())
            .bind(context.workspace_id.as_str())
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?,
        None => Some(SYSTEM_AUTHORITY.to_owned()),
    };
    observed
        .versions
        .push(json!({"kind":"creator_authority","version":version}));
    Ok(())
}
