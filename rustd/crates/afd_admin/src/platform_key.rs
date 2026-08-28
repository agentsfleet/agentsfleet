//! Platform provider-default writes; credential bytes stay in the vault.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use sqlx::{Acquire as _, Row as _};

use crate::error::{Result, query, row};

const CONTEXT_LIST: &str = "list platform keys";
const CONTEXT_SET: &str = "set platform key";
const CONTEXT_DEACTIVATE: &str = "deactivate platform key";
const DEFAULTS_TABLE: &str = "core.platform_provider_defaults";

/// Metadata selecting an existing workspace-vault key as the platform default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyInput {
    provider: String,
    source_workspace_id: Uuid7,
    model: String,
    base_url: Option<String>,
}

impl PlatformKeyInput {
    /// Builds already-validated platform-default metadata.
    #[must_use]
    pub const fn new(
        provider: String,
        source_workspace_id: Uuid7,
        model: String,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider,
            source_workspace_id,
            model,
            base_url,
        }
    }
}

/// Reveal-free platform-default metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKey {
    provider: String,
    source_workspace_id: Uuid7,
    model: Option<String>,
    active: bool,
    updated_at: UnixMillis,
}

/// Activation outcome kept distinct from datastore failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetPlatformKey {
    /// The platform default was activated.
    Set(PlatformKey),
    /// The source workspace does not exist.
    WorkspaceNotFound,
    /// The provider/model pair has no priced catalogue row.
    ModelNotFound,
}

impl PlatformKey {
    /// Provider and vault key name.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Workspace owning the vault row.
    #[must_use]
    pub const fn source_workspace_id(&self) -> &Uuid7 {
        &self.source_workspace_id
    }

    /// Active model, absent after deactivation.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Whether this is the one active default.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Last mutation instant.
    #[must_use]
    pub const fn updated_at(&self) -> UnixMillis {
        self.updated_at
    }
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
            .map(|row_value| decode(row_value, CONTEXT_LIST))
            .collect()
    }

    /// Activates one existing workspace-vault key and priced model.
    ///
    /// The key value is never an argument: rotation happens by replacing the
    /// same named vault row, while this metadata remains stable.
    ///
    /// # Errors
    /// Reports a transaction failure. Invalid references are typed outcomes and
    /// leave the previous platform default unchanged.
    pub async fn set(&self, input: &PlatformKeyInput, now: UnixMillis) -> Result<SetPlatformKey> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SET))?;
        lock_revision(&mut transaction).await?;
        let workspace_exists: bool = sqlx::query_scalar(WORKSPACE_EXISTS)
            .bind(input.source_workspace_id.as_str())
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SET))?;
        if !workspace_exists {
            return Ok(SetPlatformKey::WorkspaceNotFound);
        }
        let model_exists: bool = sqlx::query_scalar(MODEL_EXISTS)
            .bind(&input.provider)
            .bind(&input.model)
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SET))?;
        if !model_exists {
            return Ok(SetPlatformKey::ModelNotFound);
        }
        let row_value = sqlx::query(UPSERT)
            .bind(&input.provider)
            .bind(input.source_workspace_id.as_str())
            .bind(&input.model)
            .bind(&input.base_url)
            .bind(now.as_millis())
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SET))?;
        sqlx::query(DEACTIVATE_OTHERS)
            .bind(now.as_millis())
            .bind(&input.provider)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SET))?;
        let platform_key = decode(&row_value, CONTEXT_SET)?;
        transaction.commit().await.map_err(query(CONTEXT_SET))?;
        Ok(SetPlatformKey::Set(platform_key))
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

fn decode(row_value: &sqlx::postgres::PgRow, context: &'static str) -> Result<PlatformKey> {
    Ok(PlatformKey {
        provider: row_value.try_get(0).map_err(query(context))?,
        source_workspace_id: Uuid7::parse(
            &row_value.try_get::<String, _>(1).map_err(query(context))?,
        )
        .map_err(row(DEFAULTS_TABLE, "source_workspace_id"))?,
        model: row_value.try_get(2).map_err(query(context))?,
        active: row_value.try_get(3).map_err(query(context))?,
        updated_at: UnixMillis::from_millis(row_value.try_get(4).map_err(query(context))?),
    })
}

async fn lock_revision(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query("SELECT revision FROM core.model_catalogue_revision WHERE id = 1 FOR UPDATE")
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(query(CONTEXT_SET))
}

const LIST: &str = "SELECT provider, source_workspace_id::text, model, active, updated_at FROM core.platform_provider_defaults ORDER BY provider";
const WORKSPACE_EXISTS: &str = "SELECT EXISTS(SELECT 1 FROM core.workspaces WHERE id = $1::uuid)";
const MODEL_EXISTS: &str =
    "SELECT EXISTS(SELECT 1 FROM core.model_library WHERE provider = $1 AND model_id = $2)";
const UPSERT: &str = "INSERT INTO core.platform_provider_defaults (provider, source_workspace_id, model, base_url, context_cap_tokens, active, created_at, updated_at) SELECT $1, $2::uuid, $3, $4, context_cap_tokens, true, $5, $5 FROM core.model_library WHERE provider = $1 AND model_id = $3 ON CONFLICT (provider) DO UPDATE SET source_workspace_id = EXCLUDED.source_workspace_id, model = EXCLUDED.model, base_url = EXCLUDED.base_url, context_cap_tokens = EXCLUDED.context_cap_tokens, active = true, updated_at = EXCLUDED.updated_at RETURNING provider, source_workspace_id::text, model, active, updated_at";
const DEACTIVATE_OTHERS: &str = "UPDATE core.platform_provider_defaults SET active = false, model = NULL, updated_at = $1 WHERE active = true AND provider <> $2";
const DEACTIVATE: &str = "UPDATE core.platform_provider_defaults SET active = false, model = NULL, updated_at = $1 WHERE provider = $2";

#[cfg(test)]
mod tests {
    use super::{DEACTIVATE_OTHERS, LIST, UPSERT};

    #[test]
    fn platform_key_defaults_reference_vault_metadata_only() {
        for statement in [LIST, UPSERT, DEACTIVATE_OTHERS] {
            assert!(!statement.contains("api_key"));
            assert!(!statement.contains("ciphertext"));
            assert!(!statement.contains("vault.secrets"));
        }
        assert!(UPSERT.contains("source_workspace_id"));
        assert!(UPSERT.contains("core.model_library"));
        assert!(!UPSERT.contains("DEACTIVATE"));
    }
}
