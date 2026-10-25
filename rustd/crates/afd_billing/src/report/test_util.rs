//! Instrument the production bindings without exposing storage helpers in normal builds.

use afd_core::id::Uuid7;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use sqlx::{FromRow, Postgres, SqlSafeStr};

use super::Request;
use crate::biller::WorkKey;

/// Wrap the production aggregate statement, preserving its actual bindings.
pub fn aggregates_query<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
    request: &'q Request,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    super::queries::aggregates(wrap, tenant, request)
}

/// Wrap the production activity statement, preserving its actual bindings.
pub fn activities_query<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
    request: &'q Request,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    super::queries::activities(wrap, tenant, request)
}

/// Wrap the production detail statement, preserving its actual bindings.
pub fn charges_query<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
    work: &'q WorkKey,
    request: &'q Request,
    after: Option<i64>,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    super::queries::charges(wrap, tenant, work, request, after)
}
