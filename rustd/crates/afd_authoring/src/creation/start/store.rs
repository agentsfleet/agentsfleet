//! The immutable start choice and its progress use the fleet's setup identity.

use super::super::{CONTEXT, sql};
use crate::{
    Access, Result,
    error::raise::{query, stored},
    types::{InitialStart, Start},
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

pub(in crate::creation) fn key(fleet: &Uuid7) -> String {
    format!("{fleet}:setup-start")
}

pub(in crate::creation) fn payload(start: &Start) -> Result<Option<String>> {
    match start {
        Start::None => Ok(None),
        Start::RunNow { message } => serde_json::to_string(&serde_json::json!({"message":message}))
            .map(Some)
            .map_err(stored),
    }
}

pub(in crate::creation) async fn store(
    connection: &mut PgConnection,
    access: Access<'_>,
    fleet: &Uuid7,
    initial: &InitialStart,
) -> Result<()> {
    sqlx::query(sql::INITIAL)
        .bind(fleet.as_str())
        .bind(access.workspace.as_str())
        .bind(serde_json::to_string(initial).map_err(stored)?)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
