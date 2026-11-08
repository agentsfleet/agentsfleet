//! The catalogue columns every crate that shows a model decodes by name, and
//! the one writer of a model's price schedule.
//!
//! `model.platform_models` carries one identity per model and
//! `model.platform_model_prices` its dated tiers. Both are scalar columns, so
//! they decode through `FromRow` by column name: a renamed field fails the
//! decode instead of reading as a silent NULL. The tenant picker and the
//! credential registry reuse these types rather than spelling the columns
//! again.

use std::collections::HashMap;

use af_core::catalogue::{Metadata, Price};
use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::{PgConnection, PgExecutor};

use crate::{
    Result,
    error::{query, row},
    model::{DecisionModel, Model, ModelUpdate},
};
use af_crypto::entropy::Entropy;
use af_wire::{decision::DecisionConfiguration, inference::ModelInference};
use sqlx::types::Json;

const MODEL_TABLE: &str = "model.platform_models";
use crate::sql;

/// `model.platform_models`' identity columns, embedded with `#[sqlx(flatten)]`
/// in any row that selects them under these names.
#[derive(Debug, sqlx::FromRow)]
pub struct MetadataColumns {
    display_name: String,
    context_cap_tokens: i32,
    context_source_url: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    context_verified_at: Timestamp,
    advanced: bool,
}

impl From<MetadataColumns> for Metadata {
    fn from(columns: MetadataColumns) -> Self {
        Self {
            display_name: columns.display_name,
            context_cap_tokens: columns.context_cap_tokens,
            context_source_url: columns.context_source_url,
            context_verified_at: columns.context_verified_at,
            advanced: columns.advanced,
        }
    }
}

/// A `model.platform_models` row by name, before its id is parsed. Prices are
/// a separate read ([`prices_for`]); inference and decision settings remain non-secret JSON.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ModelRow {
    pub(crate) id: String,
    provider: String,
    model_name: String,
    #[sqlx(flatten)]
    metadata: MetadataColumns,
    inference: Option<Json<ModelInference>>,
    decision: Option<Json<DecisionConfiguration>>,
}

impl ModelRow {
    pub(crate) fn parse(self, prices: Vec<Price>, decision_prices: Vec<Price>) -> Result<Model> {
        Ok(Model {
            id: Uuid7::parse(&self.id).map_err(row(MODEL_TABLE, "id"))?,
            provider: self.provider,
            model_name: self.model_name,
            details: ModelUpdate {
                metadata: self.metadata.into(),
                prices,
                inference: self.inference.map(|settings| settings.0),
                decision: self.decision.map(|configuration| DecisionModel {
                    configuration: configuration.0,
                    prices: decision_prices,
                }),
            },
        })
    }
}

/// One [`sql::SELECT_MODEL_PRICES`] row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct PriceRow {
    platform_model_id: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    effective_from: Timestamp,
    effective_until: Option<jiff_sqlx::Timestamp>,
    prompt_tokens_above: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    cache_write_nanos_per_mtok: Option<i64>,
    cache_write_5m_nanos_per_mtok: Option<i64>,
    cache_write_1h_nanos_per_mtok: Option<i64>,
    output_nanos_per_mtok: i64,
    source_url: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    verified_at: Timestamp,
    conditions: String,
}

impl PriceRow {
    fn into_parts(self) -> (String, Price) {
        (
            self.platform_model_id,
            Price {
                effective_from: self.effective_from,
                effective_until: self.effective_until.map(jiff_sqlx::Timestamp::to_jiff),
                prompt_tokens_above: self.prompt_tokens_above,
                input_nanos_per_mtok: self.input_nanos_per_mtok,
                cached_input_nanos_per_mtok: self.cached_input_nanos_per_mtok,
                cache_write_nanos_per_mtok: self.cache_write_nanos_per_mtok,
                cache_write_5m_nanos_per_mtok: self.cache_write_5m_nanos_per_mtok,
                cache_write_1h_nanos_per_mtok: self.cache_write_1h_nanos_per_mtok,
                output_nanos_per_mtok: self.output_nanos_per_mtok,
                source_url: self.source_url,
                verified_at: self.verified_at,
                conditions: self.conditions,
            },
        )
    }
}

/// Some models' complete price schedules, keyed by the model's row id.
#[derive(Debug, Default)]
pub struct Prices {
    schedules: HashMap<String, Vec<Price>>,
}

/// Which protocol's currently configured schedule the catalogue is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceProtocol {
    /// Existing coding-model rates.
    Coding,
    /// Rates of each model's current reviewed decision revision.
    Decision,
}

impl Prices {
    /// Takes `model_id`'s schedule, ordered by tier then start; empty when the
    /// model has no price row.
    pub fn take(&mut self, model_id: &str) -> Vec<Price> {
        self.schedules.remove(model_id).unwrap_or_default()
    }
}

/// Reads every price row of `model_ids` in one statement and groups them.
///
/// # Errors
/// The statement's own failure, for the caller to report under its context.
pub async fn prices_for<'e>(
    executor: impl PgExecutor<'e>,
    model_ids: &[&str],
    protocol: PriceProtocol,
) -> Result<Prices, sqlx::Error> {
    let statement = match protocol {
        PriceProtocol::Coding => sql::SELECT_MODEL_PRICES,
        PriceProtocol::Decision => sql::SELECT_DECISION_PRICES,
    };
    let rows: Vec<PriceRow> = sqlx::query_as(statement)
        .bind(model_ids)
        .fetch_all(executor)
        .await?;
    let mut prices = Prices::default();
    for row in rows {
        let (model_id, price) = row.into_parts();
        prices.schedules.entry(model_id).or_default().push(price);
    }
    Ok(prices)
}

/// One replacement owns no rates: arrays borrow the caller's proved values.
struct PriceReplacement<'a> {
    model: &'a Uuid7,
    ids: &'a [String],
    prices: &'a [Price],
    decision_revision_id: Option<&'a Uuid7>,
}

impl PriceReplacement<'_> {
    async fn write(&self, connection: &mut PgConnection) -> Result<(), sqlx::Error> {
        let starts: Vec<_> = column(self.prices, |it| it.effective_from.to_sqlx());
        let tiers: Vec<_> = column(self.prices, |it| it.prompt_tokens_above);
        self.upsert(connection, &starts, &tiers).await?;
        self.prune(connection, &starts, &tiers).await
    }

    async fn upsert(
        &self,
        connection: &mut PgConnection,
        starts: &[jiff_sqlx::Timestamp],
        tiers: &[i32],
    ) -> Result<(), sqlx::Error> {
        let statement = if self.decision_revision_id.is_some() {
            sql::UPSERT_DECISION_PRICES
        } else {
            sql::UPSERT_PLATFORM_MODEL_PRICES
        };
        let mut query = sqlx::query(statement)
            .bind(self.model.as_str())
            .bind(self.ids)
            .bind(starts)
            .bind(column(self.prices, |it| {
                it.effective_until.map(jiff_sqlx::ToSqlx::to_sqlx)
            }))
            .bind(tiers)
            .bind(column(self.prices, |it| it.input_nanos_per_mtok))
            .bind(column(self.prices, |it| it.cached_input_nanos_per_mtok))
            .bind(column(self.prices, |it| it.cache_write_nanos_per_mtok))
            .bind(column(self.prices, |it| it.cache_write_5m_nanos_per_mtok))
            .bind(column(self.prices, |it| it.cache_write_1h_nanos_per_mtok))
            .bind(column(self.prices, |it| it.output_nanos_per_mtok))
            .bind(column(self.prices, |it| it.source_url.as_str()))
            .bind(column(self.prices, |it| it.verified_at.to_sqlx()))
            .bind(column(self.prices, |it| it.conditions.as_str()));
        if let Some(revision) = self.decision_revision_id {
            query = query.bind(revision.as_str());
        }
        query.execute(connection).await?;
        Ok(())
    }

    async fn prune(
        &self,
        connection: &mut PgConnection,
        starts: &[jiff_sqlx::Timestamp],
        tiers: &[i32],
    ) -> Result<(), sqlx::Error> {
        let statement = if self.decision_revision_id.is_some() {
            sql::DELETE_DECISION_PRICES
        } else {
            sql::DELETE_PLATFORM_MODEL_PRICES
        };
        let mut query = sqlx::query(statement)
            .bind(self.model.as_str())
            .bind(tiers)
            .bind(starts);
        if let Some(revision) = self.decision_revision_id {
            query = query.bind(revision.as_str());
        }
        query.execute(connection).await?;
        Ok(())
    }
}

/// One column of a price list, as the array `UNNEST` reads it.
fn column<'a, T>(prices: &'a [Price], field: impl Fn(&'a Price) -> T) -> Vec<T> {
    prices.iter().map(field).collect()
}

/// Replace both protocol schedules inside the model's mutation transaction.
pub(crate) async fn replace_schedules(
    connection: &mut PgConnection,
    entropy: &Entropy,
    model: &Uuid7,
    details: &ModelUpdate,
    now: Timestamp,
    context: &'static str,
) -> Result<()> {
    let revision = match &details.decision {
        Some(decision) => Some(
            crate::decision_revision::select(
                connection,
                entropy,
                model,
                &decision.configuration,
                now,
            )
            .await?,
        ),
        None => None,
    };
    let decision = details
        .decision
        .as_ref()
        .map(|model| (revision.as_ref(), model.prices.as_slice()));
    for (revision, prices) in std::iter::once((None, details.prices.as_slice())).chain(decision) {
        let ids = prices
            .iter()
            .map(|_| entropy.uuid7(now).map(|id| id.as_str().to_owned()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        PriceReplacement {
            model,
            ids: &ids,
            prices,
            decision_revision_id: revision,
        }
        .write(connection)
        .await
        .map_err(query(context))?;
    }
    Ok(())
}
