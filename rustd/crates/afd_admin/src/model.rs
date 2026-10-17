//! Priced model catalogue CRUD, each edit bumping the catalogue version in the
//! same transaction.

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::constraint::violates_foreign_key;
use jiff::Timestamp;
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

/// A model's mutable cap and prices, bounded by the handler's garde rules
/// before they reach this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ModelRates {
    /// Maximum model context.
    pub context_cap_tokens: i32,
    /// Input-token nanos per million tokens.
    pub input_nanos_per_mtok: i64,
    /// Cached-input nanos per million tokens.
    pub cached_input_nanos_per_mtok: i64,
    /// Output-token nanos per million tokens.
    pub output_nanos_per_mtok: i64,
}

/// A model to file. `(provider, model_name)` is immutable afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInput {
    /// Provider identifier.
    pub provider: String,
    /// The provider's name for the model.
    pub model_name: String,
    /// Cap and prices.
    pub rates: ModelRates,
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
    /// Cap and prices.
    pub rates: ModelRates,
}

/// A `model.platform_models` row by name, before its id is parsed.
#[derive(Debug, sqlx::FromRow)]
struct ModelRow {
    id: String,
    provider: String,
    model_name: String,
    #[sqlx(flatten)]
    rates: ModelRates,
}

impl ModelRow {
    fn parse(self) -> Result<Model> {
        Ok(Model {
            id: Uuid7::parse(&self.id).map_err(row(MODEL_TABLE, "id"))?,
            provider: self.provider,
            model_name: self.model_name,
            rates: self.rates,
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
    /// Reports entropy, identifier, or transaction failures.
    pub async fn create(&self, input: ModelInput, now: Timestamp) -> Result<CreateModel> {
        let id = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_CREATE))?;
        bump(&mut transaction, CONTEXT_CREATE).await?;
        let rates = input.rates;
        let done = sqlx::query(sql::INSERT_PLATFORM_MODEL)
            .bind(id.as_str())
            .bind(&input.provider)
            .bind(&input.model_name)
            .bind(rates.context_cap_tokens)
            .bind(rates.input_nanos_per_mtok)
            .bind(rates.cached_input_nanos_per_mtok)
            .bind(rates.output_nanos_per_mtok)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CREATE))?;
        if done.rows_affected() == 0 {
            return Ok(CreateModel::Duplicate);
        }
        transaction.commit().await.map_err(query(CONTEXT_CREATE))?;
        Ok(CreateModel::Created(Model {
            id,
            provider: input.provider,
            model_name: input.model_name,
            rates,
        }))
    }

    /// Replaces the cap and prices and advances the version atomically.
    ///
    /// # Errors
    /// Reports transaction failures. `Ok(false)` means no such row.
    pub async fn update(&self, id: &Uuid7, rates: ModelRates) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPDATE))?;
        bump(&mut transaction, CONTEXT_UPDATE).await?;
        let done = sqlx::query(sql::UPDATE_PLATFORM_MODEL)
            .bind(id.as_str())
            .bind(rates.context_cap_tokens)
            .bind(rates.input_nanos_per_mtok)
            .bind(rates.cached_input_nanos_per_mtok)
            .bind(rates.output_nanos_per_mtok)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
        if done.rows_affected() == 0 {
            return Ok(false);
        }
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

/// Advances the catalogue version, taking the row lock that serializes edits.
async fn bump(connection: &mut PgConnection, context: &'static str) -> Result<()> {
    sqlx::query(sql::BUMP_CATALOGUE_VERSION)
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(query(context))
}
