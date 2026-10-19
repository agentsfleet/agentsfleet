//! Platform provider-default writes; credential bytes stay in the vault.

use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::constraint::violates_foreign_key;
use jiff::Timestamp;
use sqlx::Acquire as _;

use crate::error::{Result, query, row};
use crate::sql;

const CONTEXT_LIST: &str = "list platform keys";
const CONTEXT_SET: &str = "set platform key";
const CONTEXT_DEACTIVATE: &str = "deactivate platform key";
const DEFAULTS_TABLE: &str = "model.platform_defaults";

/// The foreign key a platform default holds on its source workspace.
const SOURCE_WORKSPACE_FK: &str = "platform_defaults_source_workspace_id_fkey";

/// Metadata selecting an existing workspace-vault key as the platform default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKeyInput {
    /// Provider and vault key name.
    pub provider: String,
    /// Workspace owning the vault row.
    pub source_workspace_id: Uuid7,
    /// The catalogued model the default serves.
    pub model: String,
    /// The custom endpoint the default calls, if any.
    pub base_url: Option<String>,
}

/// Reveal-free platform-default metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformKey {
    /// Provider and vault key name.
    pub provider: String,
    /// Workspace owning the vault row.
    pub source_workspace_id: Uuid7,
    /// Active model's name, absent after deactivation.
    pub model: Option<String>,
    /// Whether this is the one active default.
    pub active: bool,
    /// Last mutation instant.
    pub updated_at: Timestamp,
}

/// A `model.platform_defaults` row joined to its model's name.
#[derive(Debug, sqlx::FromRow)]
struct DefaultRow {
    provider: String,
    source_workspace_id: String,
    model_name: Option<String>,
    active: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
}

impl DefaultRow {
    fn parse(self) -> Result<PlatformKey> {
        Ok(PlatformKey {
            provider: self.provider,
            source_workspace_id: Uuid7::parse(&self.source_workspace_id)
                .map_err(row(DEFAULTS_TABLE, "source_workspace_id"))?,
            model: self.model_name,
            active: self.active,
            updated_at: self.updated_at,
        })
    }
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
        let rows: Vec<DefaultRow> = sqlx::query_as(sql::LIST_PLATFORM_DEFAULTS)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_LIST))?;
        rows.into_iter().map(DefaultRow::parse).collect()
    }

    /// Activates one existing workspace-vault key and catalogued model.
    ///
    /// The key value is never an argument: rotation happens by replacing the
    /// same named vault row, while this metadata remains stable. The schema
    /// decides both refusals: the upsert selects the model, so an uncatalogued
    /// one writes nothing, and the source workspace's foreign key refuses a
    /// workspace with no row. Either leaves the previous default unchanged.
    ///
    /// # Errors
    /// Reports a transaction failure.
    pub async fn set(&self, input: PlatformKeyInput) -> Result<SetPlatformKey> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SET))?;
        let upserted: std::result::Result<Option<jiff_sqlx::Timestamp>, _> =
            sqlx::query_scalar(sql::UPSERT_PLATFORM_DEFAULT)
                .bind(&input.provider)
                .bind(input.source_workspace_id.as_str())
                .bind(&input.model)
                .bind(&input.base_url)
                .fetch_optional(&mut *transaction)
                .await;
        if let Err(error) = &upserted
            && violates_foreign_key(error, SOURCE_WORKSPACE_FK)
        {
            return Ok(SetPlatformKey::WorkspaceNotFound);
        }
        let Some(updated_at) = upserted.map_err(query(CONTEXT_SET))? else {
            return Ok(SetPlatformKey::ModelNotFound);
        };
        sqlx::query(sql::DEACTIVATE_OTHER_PLATFORM_DEFAULTS)
            .bind(&input.provider)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SET))?;
        transaction.commit().await.map_err(query(CONTEXT_SET))?;
        Ok(SetPlatformKey::Set(PlatformKey {
            provider: input.provider,
            source_workspace_id: input.source_workspace_id,
            model: Some(input.model),
            active: true,
            updated_at: updated_at.to_jiff(),
        }))
    }

    /// Deactivates one provider without reading its vault row.
    ///
    /// # Errors
    /// Reports a query failure.
    pub async fn deactivate(&self, provider: &str) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(sql::DEACTIVATE_PLATFORM_DEFAULT)
            .bind(provider)
            .execute(&mut *connection)
            .await
            .map(|done| done.rows_affected() != 0)
            .map_err(query(CONTEXT_DEACTIVATE))
    }
}
