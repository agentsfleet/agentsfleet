//! Priced model catalogue CRUD with revision bumps in the same transaction.

mod sql;
mod types;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::inference::ModelInference;
use sqlx::Acquire as _;

use crate::error::{Result, query};
use sql::{CREATE, DELETE, LIST, UPDATE, bind_create, bind_rates, bump, decode, lock_revision};
pub use types::{CreateModel, DeleteModel, Model, ModelInput, ModelRates};

const CONTEXT_LIST: &str = "list admin models";
const CONTEXT_CREATE: &str = "create admin model";
const CONTEXT_UPDATE: &str = "update admin model";
const CONTEXT_DELETE: &str = "delete admin model";
const MODEL_TABLE: &str = "core.model_library";

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
        sqlx::query(LIST)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_LIST))?
            .iter()
            .map(decode)
            .collect()
    }

    /// Creates a row and advances the catalogue revision atomically.
    ///
    /// # Errors
    /// Reports entropy, identifier, or transaction failures.
    pub async fn create(&self, input: &ModelInput, now: UnixMillis) -> Result<CreateModel> {
        validate(input.inference.as_ref(), input.rates)?;
        let id = self.mint(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_CREATE))?;
        lock_revision(&mut transaction, CONTEXT_CREATE).await?;
        let done = bind_create(sqlx::query(CREATE), &id, input, now)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CREATE))?;
        if done.rows_affected() == 0 {
            return Ok(CreateModel::Duplicate);
        }
        bump(&mut transaction, now, CONTEXT_CREATE).await?;
        transaction.commit().await.map_err(query(CONTEXT_CREATE))?;
        Ok(CreateModel::Created(Model {
            id,
            provider: input.provider.clone(),
            identifier: input.identifier.clone(),
            rates: input.rates,
            inference: input.inference.clone(),
        }))
    }

    /// Replaces the mutable rates and advances the revision atomically.
    ///
    /// # Errors
    /// Reports transaction failures. `Ok(false)` means no such row.
    pub async fn update(
        &self,
        id: &Uuid7,
        rates: ModelRates,
        inference: Option<&ModelInference>,
        now: UnixMillis,
    ) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPDATE))?;
        lock_revision(&mut transaction, CONTEXT_UPDATE).await?;
        let current = sql::inference(&mut transaction, id).await?;
        validate(inference.or(current.as_ref()), rates)?;
        let done = bind_rates(sqlx::query(UPDATE), id, rates, inference, now)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPDATE))?;
        if done.rows_affected() == 0 {
            return Ok(false);
        }
        bump(&mut transaction, now, CONTEXT_UPDATE).await?;
        transaction.commit().await.map_err(query(CONTEXT_UPDATE))?;
        Ok(true)
    }

    /// Deletes an unreferenced row and advances the revision atomically.
    ///
    /// # Errors
    /// Reports transaction failures.
    pub async fn delete(&self, id: &Uuid7, now: UnixMillis) -> Result<DeleteModel> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_DELETE))?;
        lock_revision(&mut transaction, CONTEXT_DELETE).await?;
        let outcome: String = sqlx::query_scalar(DELETE)
            .bind(id.as_str())
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_DELETE))?;
        let outcome = match outcome.as_str() {
            "deleted" => DeleteModel::Deleted,
            "in_use" => DeleteModel::InUse,
            _ => DeleteModel::NotFound,
        };
        if outcome != DeleteModel::Deleted {
            return Ok(outcome);
        }
        bump(&mut transaction, now, CONTEXT_DELETE).await?;
        transaction.commit().await.map_err(query(CONTEXT_DELETE))?;
        Ok(outcome)
    }

    fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }
}

fn validate(inference: Option<&ModelInference>, rates: ModelRates) -> Result<()> {
    if let Some(inference) = inference {
        let context = u32::try_from(rates.context_cap_tokens()).unwrap_or_default();
        let _configured = afd_inference::Configured::new(inference, context)?;
    }
    Ok(())
}
