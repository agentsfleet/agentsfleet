//! Priced model catalogue CRUD, each edit bumping the catalogue version in the
//! same transaction.

use afd_core::catalogue::{Metadata, Price};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::constraint::violates_foreign_key;
use afd_wire::inference::ModelInference;
use jiff::Timestamp;
use sqlx::types::Json;
use sqlx::{Acquire as _, PgConnection};

use crate::error::{Result, query, row};
use crate::sql;

const CONTEXT_LIST: &str = "list admin models";
const CONTEXT_CREATE: &str = "create admin model";
const CONTEXT_UPDATE: &str = "update admin model";
const CONTEXT_DELETE: &str = "delete admin model";
const MODEL_TABLE: &str = "model.platform_models";

/// The foreign key a platform default holds on the model it serves.
const PLATFORM_MODEL_FK: &str = "platform_defaults_platform_model_id_fkey";

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

/// A `model.platform_models` row by name, before its id is parsed.
#[derive(Debug, sqlx::FromRow)]
struct ModelRow {
    id: String,
    provider: String,
    model_name: String,
    metadata: Json<Metadata>,
    prices: Json<Vec<Price>>,
    inference: Option<Json<ModelInference>>,
}

impl ModelRow {
    fn parse(self) -> Result<Model> {
        Ok(Model {
            id: Uuid7::parse(&self.id).map_err(row(MODEL_TABLE, "id"))?,
            provider: self.provider,
            model_name: self.model_name,
            details: ModelUpdate {
                metadata: self.metadata.0,
                prices: self.prices.0,
                inference: self.inference.map(|settings| settings.0),
            },
        })
    }
}

/// Create outcome kept distinct from datastore failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateModel {
    /// The new row, including its minted identifier.
    Created(Model),
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
    database: Db,
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
        let mut connection = self.database.acquire().await?;
        let rows: Vec<ModelRow> = sqlx::query_as(sql::LIST_PLATFORM_MODELS)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_LIST))?;
        rows.into_iter().map(ModelRow::parse).collect()
    }

    /// Files a model and advances the catalogue version atomically.
    ///
    /// `now` mints the row's `UUIDv7`; the row's stamps are the database's.
    ///
    /// # Errors
    /// Refuses inference settings that cannot route within the model's
    /// context; reports entropy, identifier, or transaction failures.
    pub async fn create(&self, input: ModelInput, now: Timestamp) -> Result<CreateModel> {
        validate(input.details.inference.as_ref(), &input.details.metadata)?;
        let id = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_CREATE))?;
        bump(&mut transaction, CONTEXT_CREATE).await?;
        let metadata = &input.details.metadata;
        let done = sqlx::query(sql::INSERT_PLATFORM_MODEL)
            .bind(id.as_str())
            .bind(&input.provider)
            .bind(&input.model_name)
            .bind(Json(metadata))
            .bind(input.details.inference.as_ref().map(Json))
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CREATE))?;
        if done.rows_affected() == 0 {
            return Ok(CreateModel::Duplicate);
        }
        write_prices(&mut transaction, &id, &input.details.prices, CONTEXT_CREATE).await?;
        transaction.commit().await.map_err(query(CONTEXT_CREATE))?;
        Ok(CreateModel::Created(Model {
            id,
            provider: input.provider,
            model_name: input.model_name,
            details: input.details,
        }))
    }

    /// Replaces identity evidence and all prices, and the inference settings
    /// when `details.inference` carries new ones, advancing one version
    /// atomically.
    ///
    /// The settings that will stand, new or kept, are validated against the new
    /// context cap, so a smaller context cannot strand a saved output limit
    /// above it.
    ///
    /// # Errors
    /// Refuses settings that cannot route within the cap; reports transaction
    /// failures. `Ok(false)` means no such row.
    pub async fn update(&self, id: &Uuid7, details: ModelUpdate) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPDATE))?;
        bump(&mut transaction, CONTEXT_UPDATE).await?;
        let current: Option<Option<Json<ModelInference>>> =
            sqlx::query_scalar(sql::SELECT_PLATFORM_MODEL_INFERENCE)
                .bind(id.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(query(CONTEXT_UPDATE))?;
        let Some(current) = current else {
            return Ok(false);
        };
        validate(
            details
                .inference
                .as_ref()
                .or(current.as_ref().map(|settings| &settings.0)),
            &details.metadata,
        )?;
        let done = sqlx::query(sql::UPDATE_PLATFORM_MODEL)
            .bind(id.as_str())
            .bind(Json(&details.metadata))
            .bind(details.inference.as_ref().map(Json))
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
        if done.rows_affected() == 0 {
            return Ok(false);
        }
        write_prices(&mut transaction, id, &details.prices, CONTEXT_UPDATE).await?;
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
            .bind(id.as_str())
            .execute(&mut *transaction)
            .await;
        if let Err(error) = &deleted
            && violates_foreign_key(error, PLATFORM_MODEL_FK)
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

/// Proves saved settings route: a supported API, a safe HTTPS base and an output
/// limit inside the model's context. Absent settings are an unconfigured model.
fn validate(inference: Option<&ModelInference>, metadata: &Metadata) -> Result<()> {
    if let Some(inference) = inference {
        let context = u32::try_from(metadata.context_cap_tokens).unwrap_or_default();
        afd_inference::Configured::new(inference, context)?;
    }
    Ok(())
}

/// Retains stable price identifiers and removes only ranges absent from the replacement.
async fn write_prices(
    connection: &mut PgConnection,
    id: &Uuid7,
    prices: &[Price],
    context: &'static str,
) -> Result<()> {
    sqlx::query(sql::UPSERT_PLATFORM_MODEL_PRICES)
        .bind(id.as_str())
        .bind(Json(prices))
        .execute(&mut *connection)
        .await
        .map_err(query(context))?;
    sqlx::query(sql::DELETE_PLATFORM_MODEL_PRICES)
        .bind(id.as_str())
        .bind(Json(prices))
        .execute(connection)
        .await
        .map_err(query(context))?;
    Ok(())
}

/// Advances the catalogue version, taking the row lock that serializes edits.
async fn bump(connection: &mut PgConnection, context: &'static str) -> Result<()> {
    sqlx::query(sql::BUMP_CATALOGUE_VERSION)
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(query(context))
}
