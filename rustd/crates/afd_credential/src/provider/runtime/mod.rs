//! Runtime references preserve named and compatible providers without an authoring fallback.

mod resolve;
mod sql;

use afd_core::id::Uuid7;
use sqlx::{PgConnection, Row as _, postgres::PgRow};

use super::PLATFORM_MODEL_REF;
use crate::error::{Result, query};

const CONTEXT: &str = "resolve explicit runtime model";

/// Non-secret runtime selection shown by human setup review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeModel {
    /// Workspace owner's registry reference or the explicit platform default.
    pub reference: String,
    /// Validated provider identifier.
    pub provider: String,
    /// Model chosen by that registry entry.
    pub model: String,
    /// Runtime context limit, including its existing zero/auto meaning.
    pub context_cap_tokens: u32,
    /// Credential, endpoint, catalogue and selection generation.
    pub resolution_version: String,
}

/// Reads one owner reference with the same provider rules runtime resolution uses.
///
/// # Errors
/// Preserves query failures; a missing or unusable selection returns `None`.
pub async fn runtime_model(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RuntimeModel>> {
    read(connection, owner, reference)
        .await?
        .as_ref()
        .map(|row| metadata(row, reference))
        .transpose()
}

async fn read(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<PgRow>> {
    let statement = if reference == PLATFORM_MODEL_REF {
        sql::PLATFORM
    } else {
        sql::ENTRY
    };
    let row = sqlx::query(statement)
        .bind(owner.as_str())
        .bind(reference)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let provider: String = row.try_get(0).map_err(query(CONTEXT))?;
    let endpoint: Option<String> = row.try_get(6).map_err(query(CONTEXT))?;
    if super::validate_endpoint_pair(&provider, endpoint.as_deref()).is_err() {
        return Ok(None);
    }
    Ok(Some(row))
}

fn metadata(row: &PgRow, reference: &str) -> Result<RuntimeModel> {
    let cap: i32 = row.try_get(2).map_err(query(CONTEXT))?;
    Ok(RuntimeModel {
        reference: reference.to_owned(),
        provider: row.try_get(0).map_err(query(CONTEXT))?,
        model: row.try_get(1).map_err(query(CONTEXT))?,
        context_cap_tokens: u32::try_from(cap).unwrap_or_default(),
        resolution_version: row.try_get(3).map_err(query(CONTEXT))?,
    })
}
