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

use afd_core::catalogue::{Metadata, Price};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::{PgConnection, PgExecutor};

use crate::Result;
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
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
}

impl PriceRow {
    fn price(self) -> Price {
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
        }
    }
}

/// Some models' complete price schedules, keyed by the model's row id.
#[derive(Debug, Default)]
pub struct Prices {
    schedules: HashMap<String, Vec<Price>>,
    changed: HashMap<String, Timestamp>,
}

impl Prices {
    /// Takes `model_id`'s schedule, ordered by tier then start; empty when the
    /// model has no price row.
    pub fn take(&mut self, model_id: &str) -> Vec<Price> {
        self.schedules.remove(model_id).unwrap_or_default()
    }

    /// When any of `model_id`'s price rows last changed.
    #[must_use]
    pub fn changed(&self, model_id: &str) -> Option<Timestamp> {
        self.changed.get(model_id).copied()
    }
}

/// Reads every price row of `model_ids` in one statement and groups them.
///
/// # Errors
/// The statement's own failure, for the caller to report under its context.
pub async fn prices_for<'e>(
    executor: impl PgExecutor<'e>,
    model_ids: &[String],
) -> Result<Prices, sqlx::Error> {
    let rows: Vec<PriceRow> = sqlx::query_as(sql::SELECT_MODEL_PRICES)
        .bind(model_ids)
        .fetch_all(executor)
        .await?;
    let mut prices = Prices::default();
    for row in rows {
        let changed = prices
            .changed
            .entry(row.platform_model_id.clone())
            .or_insert(row.updated_at);
        *changed = (*changed).max(row.updated_at);
        prices
            .schedules
            .entry(row.platform_model_id.clone())
            .or_default()
            .push(row.price());
    }
    Ok(prices)
}

/// Upserts every tier of `prices` under `ids`, then removes the tiers `prices`
/// no longer carries. Each bound array is one price column, so a field renamed
/// in [`Price`] fails to compile here rather than writing a NULL.
///
/// # Errors
/// Either statement's failure, for the caller to report under its context.
pub(crate) async fn replace_prices(
    connection: &mut PgConnection,
    model: &Uuid7,
    ids: &[String],
    prices: &[Price],
) -> Result<(), sqlx::Error> {
    let starts: Vec<_> = column(prices, |it| it.effective_from.to_sqlx());
    let tiers: Vec<i32> = column(prices, |it| it.prompt_tokens_above);
    sqlx::query(sql::UPSERT_PLATFORM_MODEL_PRICES)
        .bind(model.as_str())
        .bind(ids)
        .bind(&starts)
        .bind(column(prices, |it| {
            it.effective_until.map(jiff_sqlx::ToSqlx::to_sqlx)
        }))
        .bind(&tiers)
        .bind(column(prices, |it| it.input_nanos_per_mtok))
        .bind(column(prices, |it| it.cached_input_nanos_per_mtok))
        .bind(column(prices, |it| it.cache_write_nanos_per_mtok))
        .bind(column(prices, |it| it.cache_write_5m_nanos_per_mtok))
        .bind(column(prices, |it| it.cache_write_1h_nanos_per_mtok))
        .bind(column(prices, |it| it.output_nanos_per_mtok))
        .bind(column(prices, |it| it.source_url.clone()))
        .bind(column(prices, |it| it.verified_at.to_sqlx()))
        .bind(column(prices, |it| it.conditions.clone()))
        .execute(&mut *connection)
        .await?;
    sqlx::query(sql::DELETE_PLATFORM_MODEL_PRICES)
        .bind(model.as_str())
        .bind(&tiers)
        .bind(&starts)
        .execute(connection)
        .await?;
    Ok(())
}

/// One column of a price list, as the array `UNNEST` reads it.
fn column<T>(prices: &[Price], field: impl Fn(&Price) -> T) -> Vec<T> {
    prices.iter().map(field).collect()
}
