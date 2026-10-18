//! Runtime references preserve named and compatible providers without an authoring fallback.

mod resolve;
mod sql;

use afd_core::id::Uuid7;
use sqlx::PgConnection;

use super::{OPENAI_COMPATIBLE, PLATFORM_MODEL_REF};
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

/// One [`sql::ENTRY`] or [`sql::PLATFORM`] row, by the names both give.
#[derive(Debug, sqlx::FromRow)]
struct RuntimeRow {
    provider: String,
    model: String,
    context_cap_tokens: i32,
    resolution_version: String,
    workspace_id: String,
    key_name: String,
    endpoint: Option<String>,
}

impl RuntimeRow {
    /// The non-secret selection, as read for `reference`.
    fn metadata(&self, reference: &str) -> RuntimeModel {
        RuntimeModel {
            reference: reference.to_owned(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            context_cap_tokens: u32::try_from(self.context_cap_tokens).unwrap_or_default(),
            resolution_version: self.resolution_version.clone(),
        }
    }
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
    Ok(read(connection, owner, reference)
        .await?
        .map(|row| row.metadata(reference)))
}

/// The row for `reference`, or nothing when it is absent or its provider and
/// endpoint do not pair.
async fn read(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RuntimeRow>> {
    let statement = if reference == PLATFORM_MODEL_REF {
        sql::PLATFORM
    } else {
        sql::ENTRY
    };
    let row = sqlx::query_as::<_, RuntimeRow>(statement)
        .bind(owner.as_str())
        .bind(reference)
        .bind(OPENAI_COMPATIBLE)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(row.filter(|row| {
        super::validate_endpoint_pair(&row.provider, row.endpoint.as_deref()).is_ok()
    }))
}
