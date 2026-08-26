//! Platform provider-default writes; credential bytes stay in the vault.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use sqlx::Row as _;

use crate::error::{Result, query, row_malformed};

const CONTEXT_LIST: &str = "list platform keys";
const CONTEXT_SET: &str = "set platform key";
const CONTEXT_DEACTIVATE: &str = "deactivate platform key";

/// Metadata selecting an existing workspace-vault key as the platform default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyInput {
    /// Vault key name and provider identifier.
    pub provider: String,
    /// Workspace whose vault owns the provider key.
    pub source_workspace_id: Uuid7,
    /// Priced model identifier.
    pub model: String,
    /// Optional custom provider endpoint.
    pub base_url: Option<String>,
}

/// Reveal-free platform-default metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKey {
    /// Provider and vault key name.
    pub provider: String,
    /// Workspace owning the vault row.
    pub source_workspace_id: Uuid7,
    /// Active model, absent after deactivation.
    pub model: Option<String>,
    /// Whether this is the one active default.
    pub active: bool,
    /// Last mutation instant.
    pub updated_at: UnixMillis,
}

/// Platform-default metadata store.
#[derive(Debug, Clone)]
pub struct PlatformKeys(Db);

impl PlatformKeys {
    /// Uses the already-connected API-role pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self(database)
    }

    /// Lists metadata only; this repository has no vault-read capability.
    ///
    /// # Errors
    /// Reports a query or malformed-row failure.
    pub async fn list(&self) -> Result<Vec<PlatformKey>> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(LIST)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_LIST))?
            .iter()
            .map(row)
            .collect()
    }

    /// Activates one existing workspace-vault key and priced model.
    ///
    /// The key value is never an argument: rotation happens by replacing the
    /// same named vault row, while this metadata remains stable.
    ///
    /// # Errors
    /// Reports a query failure. Returns `Ok(None)` when the workspace or priced
    /// provider/model pair does not exist.
    pub async fn set(&self, input: &PlatformKeyInput, now: UnixMillis) -> Result<Option<PlatformKey>> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(SET)
            .bind(&input.provider)
            .bind(input.source_workspace_id.as_str())
            .bind(&input.model)
            .bind(&input.base_url)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SET))?
            .as_ref()
            .map(row)
            .transpose()
    }

    /// Deactivates one provider without reading its vault row.
    ///
    /// # Errors
    /// Reports a query failure.
    pub async fn deactivate(&self, provider: &str, now: UnixMillis) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(DEACTIVATE)
            .bind(now.as_millis())
            .bind(provider)
            .execute(&mut *connection)
            .await
            .map(|done| done.rows_affected() != 0)
            .map_err(query(CONTEXT_DEACTIVATE))
    }
}

fn row(row: &sqlx::postgres::PgRow) -> Result<PlatformKey> {
    Ok(PlatformKey {
        provider: row.try_get(0).map_err(query(CONTEXT_LIST))?,
        source_workspace_id: Uuid7::parse(
            &row.try_get::<String, _>(1).map_err(query(CONTEXT_LIST))?,
        )
        .map_err(row_malformed(
            "core.platform_provider_defaults",
            "source_workspace_id",
        ))?,
        model: row.try_get(2).map_err(query(CONTEXT_LIST))?,
        active: row.try_get(3).map_err(query(CONTEXT_LIST))?,
        updated_at: UnixMillis::from_millis(row.try_get(4).map_err(query(CONTEXT_LIST))?),
    })
}

const LIST: &str = "SELECT provider, source_workspace_id::text, model, active, updated_at FROM core.platform_provider_defaults ORDER BY provider";

const SET: &str = "WITH chosen AS (SELECT m.context_cap_tokens FROM core.model_library m JOIN core.workspaces w ON w.id = $2::uuid WHERE m.provider = $1 AND m.model_id = $3), deactivated AS (UPDATE core.platform_provider_defaults SET active = false, model = NULL, updated_at = $5 WHERE active = true AND provider <> $1), upserted AS (INSERT INTO core.platform_provider_defaults (provider, source_workspace_id, model, base_url, context_cap_tokens, active, created_at, updated_at) SELECT $1, $2::uuid, $3, $4, context_cap_tokens, true, $5, $5 FROM chosen ON CONFLICT (provider) DO UPDATE SET source_workspace_id = EXCLUDED.source_workspace_id, model = EXCLUDED.model, base_url = EXCLUDED.base_url, context_cap_tokens = EXCLUDED.context_cap_tokens, active = true, updated_at = EXCLUDED.updated_at RETURNING provider, source_workspace_id::text, model, active, updated_at) SELECT provider, source_workspace_id, model, active, updated_at FROM upserted";

const DEACTIVATE: &str = "UPDATE core.platform_provider_defaults SET active = false, model = NULL, updated_at = $1 WHERE provider = $2";

#[cfg(test)]
mod tests {
    use super::{LIST, SET};

    #[test]
    fn test_platform_key_vault_semantics() {
        for statement in [LIST, SET] {
            assert!(!statement.contains("api_key"));
            assert!(!statement.contains("ciphertext"));
            assert!(!statement.contains("vault.secrets"));
        }
        assert!(SET.contains("source_workspace_id"));
        assert!(SET.contains("core.model_library"));
    }
}
