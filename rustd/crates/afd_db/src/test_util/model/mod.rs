//! The priced `model.platform_models` row a suite seeds, and its removal.
//!
//! [`super::DefaultSeed`] writes a zero-priced catalogue row when none exists,
//! which is all a suite resolving a provider needs. A suite that METERS needs
//! real prices, so it seeds its row first with a [`PlatformModelSeed`] and the
//! default then names that row as it stands. The row is deployment-wide, keyed
//! by provider and model name, so the suite that wrote it removes it by the id
//! it seeded the row under.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a platform model seed sets.
const INSERT_PLATFORM_MODEL: &str = include_str!("sql/insert_platform_model.sql");

/// Removes one row by the id its seed answered.
const DELETE_PLATFORM_MODEL: &str = include_str!("sql/delete_platform_model.sql");

/// The inference settings every seeded platform model carries, so a seeded
/// model routes: without saved settings the registered read finds no route.
/// The output limit stays under the smallest context a suite seeds.
pub(super) const FIXTURE_INFERENCE: &str =
    r#"{"api":"messages","base_url":"https://api.anthropic.com/v1","max_output_tokens":1024}"#;

const CONTEXT_INSERT: &str = "seed a platform model";
const CONTEXT_DELETE: &str = "remove a platform model";

/// What one million tokens cost on a model, in nanos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prices {
    /// Uncached input.
    pub input_nanos_per_mtok: i64,
    /// Input read from the provider's cache.
    pub cached_input_nanos_per_mtok: i64,
    /// Output.
    pub output_nanos_per_mtok: i64,
}

/// One priced catalogue row. Stamped by the database's `now()` unless the
/// suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct PlatformModelSeed<'a> {
    id: &'a str,
    provider: &'a str,
    model_name: &'a str,
    context_cap_tokens: i32,
    prices: Prices,
    created_at: Option<Timestamp>,
}

impl<'a> PlatformModelSeed<'a> {
    /// `model_name` under `provider` as row `id`, with its context ceiling and
    /// prices.
    pub const fn new(
        id: &'a str,
        provider: &'a str,
        model_name: &'a str,
        context_cap_tokens: i32,
        prices: Prices,
    ) -> Self {
        Self {
            id,
            provider,
            model_name,
            context_cap_tokens,
            prices,
            created_at: None,
        }
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a model the catalogue already
    /// carries under this provider among them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_PLATFORM_MODEL)
            .bind(self.id)
            .bind(self.model_name)
            .bind(self.provider)
            .bind(self.context_cap_tokens)
            .bind(self.prices.input_nanos_per_mtok)
            .bind(self.prices.cached_input_nanos_per_mtok)
            .bind(self.prices.output_nanos_per_mtok)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .bind(FIXTURE_INFERENCE)
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_INSERT, source))?;
        Ok(())
    }

    /// Removes the row `id` names. A row already gone is not an error.
    ///
    /// # Errors
    /// [`crate::Error`] when the delete fails, a platform default still naming
    /// the row among them: remove the default first.
    pub async fn remove<'e>(executor: impl PgExecutor<'e>, id: &str) -> Result<()> {
        sqlx::query(DELETE_PLATFORM_MODEL)
            .bind(id)
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT_DELETE, source))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{INSERT_PLATFORM_MODEL, PlatformModelSeed, Prices};

    const PRICES: Prices = Prices {
        input_nanos_per_mtok: 3,
        cached_input_nanos_per_mtok: 1,
        output_nanos_per_mtok: 15,
    };

    #[test]
    fn a_seed_stamps_nothing_until_the_suite_backdates_it() {
        let seed = PlatformModelSeed::new("row", "provider", "model", 8, PRICES);

        assert_eq!((seed.prices, seed.created_at), (PRICES, None));
    }

    #[test]
    fn the_seed_writes_the_catalogue_table_by_its_schema() {
        assert!(
            INSERT_PLATFORM_MODEL.starts_with("INSERT INTO model.platform_models"),
            "{INSERT_PLATFORM_MODEL}"
        );
    }
}
