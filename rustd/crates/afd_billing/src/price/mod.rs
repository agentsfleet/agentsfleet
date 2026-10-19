//! What a request costs: the catalogue's dated tiers for one model, and the pure
//! [`Rater`] that turns one usage line into nanos.
//!
//! Pricing is split from money movement on purpose (billing §11): every biller
//! receives the same priced lines, whichever system moves the money. Nothing in
//! [`rate`] reaches a datastore, so the arithmetic is proven without one.

use afd_core::catalogue::Price;
use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::PgExecutor;

use crate::error::{query, row_malformed};
use crate::{Result, sql};

mod rate;

pub use self::rate::{RatedLine, Rater, Window};

/// What a statement here was doing, for [`query`].
const CONTEXT_LOAD: &str = "read a model's price book";
/// The table a malformed price id is reported against.
const TABLE_PRICES: &str = "model.platform_model_prices";
/// The column a malformed price id is reported against.
const COLUMN_ID: &str = "id";

/// One dated prompt-size tier of one model's prices, with its row id.
///
/// The id is what a usage line records as the price it was charged at, so a
/// charge can always be traced to the exact published rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// `model.platform_model_prices.id`.
    pub id: Uuid7,
    /// The rates and their evidence.
    pub price: Price,
}

impl Tier {
    /// Whether this tier's date range covers `at`: inclusive start, exclusive end.
    fn covers(&self, at: Timestamp) -> bool {
        self.price.effective_from <= at && self.price.effective_until.is_none_or(|until| at < until)
    }

    /// Whether a request of `whole_prompt` tokens is priced at this tier.
    ///
    /// The base tier (threshold zero) takes every request; any other tier takes
    /// a request whose whole prompt is ABOVE its threshold.
    fn admits(&self, whole_prompt: u64) -> bool {
        let threshold = i128::from(self.price.prompt_tokens_above);
        threshold == 0 || i128::from(whole_prompt) > threshold
    }
}

/// Every published tier of one model, across all its dates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriceBook {
    tiers: Vec<Tier>,
}

impl PriceBook {
    /// A book over `tiers`, in any order.
    #[must_use]
    pub const fn from_tiers(tiers: Vec<Tier>) -> Self {
        Self { tiers }
    }

    /// The tiers `provider`'s `model` is published at.
    ///
    /// A model the catalogue does not carry answers an empty book, which prices
    /// every token as unknown rather than free.
    ///
    /// # Errors
    /// Reports a statement that would not run or a row this build cannot read.
    pub async fn load<'e>(
        executor: impl PgExecutor<'e>,
        provider: &str,
        model: &str,
    ) -> Result<Self> {
        let rows: Vec<PriceColumns> = sqlx::query_as(sql::SELECT_PRICE_BOOK)
            .bind(provider)
            .bind(model)
            .fetch_all(executor)
            .await
            .map_err(query(CONTEXT_LOAD))?;
        let tiers = rows
            .into_iter()
            .map(PriceColumns::tier)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { tiers })
    }

    /// The tier a request of `whole_prompt` tokens made at `at` is priced at.
    ///
    /// Of the tiers whose dates cover `at`, the highest threshold the request
    /// exceeds: a context tier prices the WHOLE request, never only its tokens
    /// past the threshold. `None` when no published range covers `at`.
    #[must_use]
    pub fn quote(&self, at: Timestamp, whole_prompt: u64) -> Option<&Tier> {
        self.tiers
            .iter()
            .filter(|tier| tier.covers(at) && tier.admits(whole_prompt))
            .max_by_key(|tier| tier.price.prompt_tokens_above)
    }
}

/// `model.platform_model_prices` read by column name.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct PriceColumns {
    id: String,
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

impl PriceColumns {
    /// The row as a [`Tier`].
    ///
    /// # Errors
    /// The row's id is not a `UUIDv7`.
    pub(crate) fn tier(self) -> Result<Tier> {
        let id = Uuid7::parse(&self.id).map_err(row_malformed(TABLE_PRICES, COLUMN_ID))?;
        Ok(Tier {
            id,
            price: Price {
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
        })
    }
}

#[cfg(test)]
mod tests;
