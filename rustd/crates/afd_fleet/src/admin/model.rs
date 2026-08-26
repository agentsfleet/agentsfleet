//! Priced model catalogue CRUD with revision bumps in the same transaction.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use sqlx::{Acquire as _, Row as _};

use crate::error::{Result, query, row_malformed};

const CONTEXT_LIST: &str = "list admin models";
const CONTEXT_CREATE: &str = "create admin model";
const CONTEXT_UPDATE: &str = "update admin model";
const CONTEXT_DELETE: &str = "delete admin model";
const MODEL_TABLE: &str = "core.model_library";

/// Mutable priced-model fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Create input whose provider/model identity is immutable afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInput {
    /// Provider identifier.
    pub provider: String,
    /// Provider-native model identifier.
    pub model_id: String,
    /// Mutable rates and cap.
    pub rates: ModelRates,
}

/// One priced model row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// Opaque row identifier.
    pub id: Uuid7,
    /// Provider identifier.
    pub provider: String,
    /// Provider-native model identifier.
    pub model_id: String,
    /// Mutable rates and cap.
    pub rates: ModelRates,
}

/// Delete outcome kept distinct from datastore failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteModel {
    /// Row was removed and the revision advanced.
    Deleted,
    /// No row matches the identifier.
    NotFound,
    /// The active platform default still references this row.
    InUse,
}

/// Model catalogue repository.
#[derive(Debug, Clone)]
pub struct Models(Db);

impl Models {
    /// Uses the already-connected API-role pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self(database)
    }

    /// Lists every priced row in stable provider/model order.
    ///
    /// # Errors
    /// Reports a query or malformed-row failure.
    pub async fn list(&self) -> Result<Vec<Model>> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(LIST).fetch_all(&mut *connection).await
            .map_err(query(CONTEXT_LIST))?.iter().map(row).collect()
    }

    /// Creates a row and advances the catalogue revision atomically.
    ///
    /// # Errors
    /// Reports transaction failures. `Ok(false)` means duplicate identity.
    pub async fn create(&self, id: &Uuid7, input: &ModelInput, now: UnixMillis) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_CREATE))?;
        lock_revision(&mut transaction, CONTEXT_CREATE).await?;
        let done = bind_create(sqlx::query(CREATE), id, input, now)
            .execute(&mut *transaction).await.map_err(query(CONTEXT_CREATE))?;
        if done.rows_affected() == 0 { return Ok(false); }
        bump(&mut transaction, now, CONTEXT_CREATE).await?;
        transaction.commit().await.map_err(query(CONTEXT_CREATE))?;
        Ok(true)
    }

    /// Replaces the mutable rates and advances the revision atomically.
    ///
    /// # Errors
    /// Reports transaction failures. `Ok(false)` means no such row.
    pub async fn update(&self, id: &Uuid7, rates: ModelRates, now: UnixMillis) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPDATE))?;
        lock_revision(&mut transaction, CONTEXT_UPDATE).await?;
        let done = bind_rates(sqlx::query(UPDATE), id, rates, now)
            .execute(&mut *transaction).await.map_err(query(CONTEXT_UPDATE))?;
        if done.rows_affected() == 0 { return Ok(false); }
        bump(&mut transaction, now, CONTEXT_UPDATE).await?;
        transaction.commit().await.map_err(query(CONTEXT_UPDATE))?;
        Ok(true)
    }

    /// Deletes an unreferenced row and advances the revision atomically.
    ///
    /// # Errors
    /// Reports transaction failures.
    pub async fn delete(&self, id: &Uuid7, now: UnixMillis) -> Result<DeleteModel> {
        let mut connection = self.0.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_DELETE))?;
        lock_revision(&mut transaction, CONTEXT_DELETE).await?;
        let outcome: String = sqlx::query_scalar(DELETE).bind(id.as_str())
            .fetch_one(&mut *transaction).await.map_err(query(CONTEXT_DELETE))?;
        let outcome = match outcome.as_str() { "deleted" => DeleteModel::Deleted, "in_use" => DeleteModel::InUse, _ => DeleteModel::NotFound };
        if outcome != DeleteModel::Deleted { return Ok(outcome); }
        bump(&mut transaction, now, CONTEXT_DELETE).await?;
        transaction.commit().await.map_err(query(CONTEXT_DELETE))?;
        Ok(outcome)
    }
}

fn row(row: &sqlx::postgres::PgRow) -> Result<Model> {
    let id: String = row.try_get(0).map_err(query(CONTEXT_LIST))?;
    Ok(Model {
        id: Uuid7::parse(&id).map_err(row_malformed(MODEL_TABLE, "id"))?,
        provider: row.try_get(1).map_err(query(CONTEXT_LIST))?,
        model_id: row.try_get(2).map_err(query(CONTEXT_LIST))?,
        rates: ModelRates { context_cap_tokens: row.try_get(3).map_err(query(CONTEXT_LIST))?, input_nanos_per_mtok: row.try_get(4).map_err(query(CONTEXT_LIST))?, cached_input_nanos_per_mtok: row.try_get(5).map_err(query(CONTEXT_LIST))?, output_nanos_per_mtok: row.try_get(6).map_err(query(CONTEXT_LIST))? },
    })
}

fn bind_create<'a>(query: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>, id: &'a Uuid7, input: &'a ModelInput, now: UnixMillis) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query.bind(id.as_str()).bind(&input.model_id).bind(&input.provider).bind(input.rates.context_cap_tokens).bind(input.rates.input_nanos_per_mtok).bind(input.rates.cached_input_nanos_per_mtok).bind(input.rates.output_nanos_per_mtok).bind(now.as_millis())
}

fn bind_rates<'a>(query: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>, id: &'a Uuid7, rates: ModelRates, now: UnixMillis) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query.bind(id.as_str()).bind(rates.context_cap_tokens).bind(rates.input_nanos_per_mtok).bind(rates.cached_input_nanos_per_mtok).bind(rates.output_nanos_per_mtok).bind(now.as_millis())
}

async fn lock_revision(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, context: &'static str) -> Result<()> {
    sqlx::query("SELECT revision FROM core.model_catalogue_revision WHERE id = 1 FOR UPDATE").execute(&mut **transaction).await.map(|_| ()).map_err(query(context))
}

async fn bump(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, now: UnixMillis, context: &'static str) -> Result<()> {
    sqlx::query("UPDATE core.model_catalogue_revision SET revision = revision + 1, updated_at = $1 WHERE id = 1").bind(now.as_millis()).execute(&mut **transaction).await.map(|_| ()).map_err(query(context))
}

const LIST: &str = "SELECT id::text, provider, model_id, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok FROM core.model_library ORDER BY provider, model_id";
const CREATE: &str = "INSERT INTO core.model_library (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) VALUES ($1::uuid,$2,$3,$4,$5,$6,$7,$8,$8) ON CONFLICT (provider, model_id) DO NOTHING";
const UPDATE: &str = "UPDATE core.model_library SET context_cap_tokens=$2,input_nanos_per_mtok=$3,cached_input_nanos_per_mtok=$4,output_nanos_per_mtok=$5,updated_at=$6 WHERE id=$1::uuid";
const DELETE: &str = "WITH target AS (SELECT provider,model_id FROM core.model_library WHERE id=$1::uuid), blocked AS (SELECT 1 FROM target t JOIN core.platform_provider_defaults p ON p.provider=t.provider AND p.model=t.model_id AND p.active=true), removed AS (DELETE FROM core.model_library WHERE id=$1::uuid AND NOT EXISTS (SELECT 1 FROM blocked) RETURNING 1) SELECT CASE WHEN EXISTS(SELECT 1 FROM removed) THEN 'deleted' WHEN EXISTS(SELECT 1 FROM blocked) THEN 'in_use' ELSE 'not_found' END";
