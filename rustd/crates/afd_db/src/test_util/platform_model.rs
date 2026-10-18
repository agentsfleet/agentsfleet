//! Synthetic catalogue identities and current base quotes for integration suites.
//!
//! The fixture evidence URLs identify test data, never a provider API response.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

const INSERT: &str = include_str!("sql/insert_platform_model.sql");
const CONTEXT: &str = "seed a platform model";

/// One synthetic identity and three base rates, with SQL-owned storage clocks.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until insert runs"]
pub struct ModelSeed<'a> {
    id: &'a str,
    provider: &'a str,
    model: &'a str,
    context: i32,
    input: i64,
    cached: i64,
    output: i64,
    verified_at: Option<Timestamp>,
}

impl<'a> ModelSeed<'a> {
    /// Uses the model identifier as the fixture label and zero test-only rates.
    pub const fn new(id: &'a str, provider: &'a str, model: &'a str, context: i32) -> Self {
        Self {
            id,
            provider,
            model,
            context,
            input: 0,
            cached: 0,
            output: 0,
            verified_at: None,
        }
    }

    /// Sets exact integer nanodollars per million tokens.
    pub const fn rates(mut self, input: i64, cached: i64, output: i64) -> Self {
        self.input = input;
        self.cached = cached;
        self.output = output;
        self
    }

    /// Sets the synthetic evidence time; SQL still owns creation/update clocks.
    pub const fn verified_at(mut self, instant: Timestamp) -> Self {
        self.verified_at = Some(instant);
        self
    }

    /// Upserts one identity and its epoch-start base quote atomically.
    ///
    /// # Errors
    /// Preserves a failed statement's `SQLx` source through the database error.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT)
            .bind(self.id)
            .bind(self.provider)
            .bind(self.model)
            .bind(self.context)
            .bind(self.input)
            .bind(self.cached)
            .bind(self.output)
            .bind(self.verified_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
        Ok(())
    }
}
