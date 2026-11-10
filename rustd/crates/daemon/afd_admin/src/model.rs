//! Priced model catalogue CRUD, each edit bumping the catalogue version in the
//! same transaction.

use af_core::catalogue::{Metadata, Price};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::decision::DecisionConfiguration;
use af_wire::inference::ModelInference;
use afd_db::Db;
use afd_db::constraint::violates_foreign_key;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::types::Json;
use sqlx::{Acquire as _, PgConnection};

use crate::catalogue::replace_schedules;
use crate::error::{Result, query};
use crate::model_validation::{CurrentSettings, validate};
use crate::sql;

const CONTEXT_CREATE: &str = "create admin model";
const CONTEXT_UPDATE: &str = "update admin model";
const CONTEXT_DELETE: &str = "delete admin model";

/// The foreign key a platform default holds on the model it serves.
const PLATFORM_MODEL_FK: &str = "platform_defaults_platform_model_id_fkey";
const RUNNER_DECISION_MODEL_FK: &str = "runners_decision_platform_model_id_fkey";

/// A reviewed decision definition and its independent protocol schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionModel {
    /// Activation and immutable reviewed profile metadata.
    pub configuration: DecisionConfiguration,
    /// Complete schedule for this decision revision, separate from coding rates.
    pub prices: Vec<Price>,
}

/// One replacement, validated by the HTTP handler's garde input types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUpdate {
    /// Labels, verified context and advanced-selection flag.
    pub metadata: Metadata,
    /// Complete set of prompt tiers and date ranges, with price evidence.
    pub prices: Vec<Price>,
    /// Non-secret transport settings. On create, absent files an unconfigured
    /// model; on update, absent keeps the saved settings.
    pub inference: Option<ModelInference>,
    /// Absent preserves the saved decision definition and schedule on update.
    pub decision: Option<DecisionModel>,
}

/// A model to file. `(provider, model_name)` is immutable afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInput {
    /// Provider identifier.
    pub provider: String,
    /// The provider's name for the model.
    pub model_name: String,
    /// Identity evidence, complete price set and inference settings.
    pub details: ModelUpdate,
}

/// One priced model row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Opaque row identifier.
    pub id: Uuid7,
    /// Provider identifier.
    pub provider: String,
    /// The provider's name for the model.
    pub model_name: String,
    /// Identity evidence, complete price set and inference settings.
    pub details: ModelUpdate,
}

/// Create outcome kept distinct from datastore failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateModel {
    /// The new row, including its minted identifier; boxed because a price
    /// schedule makes it far larger than the duplicate answer beside it.
    Created(Box<Model>),
    /// A row already has the provider/model identity.
    Duplicate,
}

/// Delete outcome kept distinct from datastore failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteModel {
    /// Row was removed and the version advanced.
    Deleted,
    /// No row matches the identifier.
    NotFound,
    /// A platform default still points at this row.
    InUse,
}

/// Model catalogue repository.
#[derive(Debug, Clone)]
pub struct Models {
    pub(crate) database: Db,
    entropy: Entropy,
}

impl Models {
    /// Uses the already-connected API-role pool and shared entropy surface.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Lists every priced row in stable provider/model order.
    ///
    /// # Errors
    /// Reports a query or malformed-row failure.
    pub async fn list(&self) -> Result<Vec<Model>> {
        self.read(None, None).await.map(|(rows, _)| rows)
    }

    /// Reads at most `limit` catalogue entries and reports a clipped snapshot.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn bounded(&self, limit: u32) -> Result<(Vec<Model>, bool)> {
        let mut rows = self.read(Some(i64::from(limit) + 1), None).await?.0;
        let kept = usize::try_from(limit).unwrap_or(usize::MAX);
        let clipped = rows.len() > kept;
        rows.truncate(kept);
        Ok((rows, clipped))
    }

    /// Files a model and advances the catalogue version atomically.
    ///
    /// `now` mints the row's `UUIDv7`; the row's stamps are the database's.
    ///
    /// # Errors
    /// Refuses inference settings that cannot route within the model's
    /// context; reports entropy, identifier, or transaction failures.
    pub async fn create(&self, input: ModelInput, now: Timestamp) -> Result<CreateModel> {
        let decision = input
            .details
            .decision
            .as_ref()
            .map(|model| &model.configuration);
        validate(
            input.details.inference.as_ref(),
            &input.details.metadata,
            decision,
            &input.details.prices,
        )?;
        let id = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_CREATE))?;
        bump(&mut transaction, CONTEXT_CREATE).await?;
        let metadata = &input.details.metadata;
        let done = sqlx::query(sql::INSERT_PLATFORM_MODEL)
            .bind(&id)
            .bind(&input.provider)
            .bind(&input.model_name)
            .bind(&metadata.display_name)
            .bind(metadata.context_cap_tokens)
            .bind(&metadata.context_source_url)
            .bind(metadata.context_verified_at.to_sqlx())
            .bind(metadata.advanced)
            .bind(input.details.inference.as_ref().map(Json))
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CREATE))?;
        if done.rows_affected() == 0 {
            return Ok(CreateModel::Duplicate);
        }
        replace_schedules(
            &mut transaction,
            &self.entropy,
            &id,
            &input.details,
            now,
            CONTEXT_CREATE,
        )
        .await?;
        transaction.commit().await.map_err(query(CONTEXT_CREATE))?;
        Ok(CreateModel::Created(Box::new(Model {
            id,
            provider: input.provider,
            model_name: input.model_name,
            details: input.details,
        })))
    }

    /// Replaces identity evidence and all prices, and the inference settings
    /// when `details.inference` carries new ones, advancing one version
    /// atomically.
    ///
    /// The settings that will stand, new or kept, are validated against the new
    /// context cap, so a smaller context cannot strand a saved output limit
    /// above it.
    ///
    /// `now` mints the identifiers of price rows the replacement adds.
    ///
    /// # Errors
    /// Refuses settings that cannot route within the cap; reports entropy and
    /// transaction failures. `Ok(false)` means no such row.
    pub async fn update(&self, id: &Uuid7, details: ModelUpdate, now: Timestamp) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPDATE))?;
        bump(&mut transaction, CONTEXT_UPDATE).await?;
        let current: Option<CurrentSettings> = sqlx::query_as(sql::SELECT_PLATFORM_MODEL_INFERENCE)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
        let Some(current) = current else {
            return Ok(false);
        };
        current.validate_replacement(&details)?;
        sqlx::query(sql::UPDATE_PLATFORM_MODEL)
            .bind(id)
            .bind(&details.metadata.display_name)
            .bind(details.metadata.context_cap_tokens)
            .bind(&details.metadata.context_source_url)
            .bind(details.metadata.context_verified_at.to_sqlx())
            .bind(details.metadata.advanced)
            .bind(details.inference.as_ref().map(Json))
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
        replace_schedules(
            &mut transaction,
            &self.entropy,
            id,
            &details,
            now,
            CONTEXT_UPDATE,
        )
        .await?;
        transaction.commit().await.map_err(query(CONTEXT_UPDATE))?;
        Ok(true)
    }

    /// Deletes a model no platform default points at, and advances the
    /// version atomically.
    ///
    /// # Errors
    /// Reports transaction failures.
    pub async fn delete(&self, id: &Uuid7) -> Result<DeleteModel> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_DELETE))?;
        bump(&mut transaction, CONTEXT_DELETE).await?;
        let deleted = sqlx::query(sql::DELETE_PLATFORM_MODEL)
            .bind(id)
            .execute(&mut *transaction)
            .await;
        if let Err(error) = &deleted
            && (violates_foreign_key(error, PLATFORM_MODEL_FK)
                || violates_foreign_key(error, RUNNER_DECISION_MODEL_FK))
        {
            return Ok(DeleteModel::InUse);
        }
        if deleted.map_err(query(CONTEXT_DELETE))?.rows_affected() == 0 {
            return Ok(DeleteModel::NotFound);
        }
        transaction.commit().await.map_err(query(CONTEXT_DELETE))?;
        Ok(DeleteModel::Deleted)
    }
}

/// Advances the catalogue version, taking the row lock that serializes edits.
async fn bump(connection: &mut PgConnection, context: &'static str) -> Result<()> {
    sqlx::query(sql::BUMP_CATALOGUE_VERSION)
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(query(context))
}
