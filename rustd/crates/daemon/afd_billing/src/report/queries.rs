//! Production bindings shared with the real-query plan proofs.

use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use sqlx::{FromRow, Postgres, SqlSafeStr};

use super::{Request, dimension};

/// Full-period aggregates and filter choices, with the production bindings.
pub(super) fn aggregates<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
    request: &'q Request,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    sqlx::query_as(wrap(include_str!("sql/aggregates.sql")))
        .bind(tenant)
        .bind(request.window.from.to_sqlx())
        .bind(request.window.through.to_sqlx())
        .bind(request.filter.fleet.as_ref())
        .bind(request.filter.work_type)
        .bind(dimension::SUMMARY)
        .bind(dimension::FLEET)
        .bind(dimension::MODEL)
        .bind(dimension::DAY)
}
