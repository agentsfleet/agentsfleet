//! Production bindings shared with the real-query plan proofs.

use afd_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use sqlx::{FromRow, Postgres, SqlSafeStr};

use super::{Request, dimension};
use crate::biller::WorkKey;

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
        .bind(tenant.as_str())
        .bind(request.window.from.to_sqlx())
        .bind(request.window.through.to_sqlx())
        .bind(request.filter.fleet.as_ref().map(Uuid7::as_str))
        .bind(request.filter.model.as_deref())
        .bind(request.filter.work_type)
        .bind(dimension::SUMMARY)
        .bind(dimension::FLEET)
        .bind(dimension::MODEL)
}

/// Grouped activity paging, with every tie-breaking key in its boundary.
pub(super) fn activities<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
    request: &'q Request,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    sqlx::query_as(wrap(include_str!("sql/activities.sql")))
        .bind(tenant.as_str())
        .bind(request.window.from.to_sqlx())
        .bind(request.window.through.to_sqlx())
        .bind(request.filter.fleet.as_ref().map(Uuid7::as_str))
        .bind(request.filter.model.as_deref())
        .bind(request.filter.work_type)
        .bind(
            request
                .after
                .as_ref()
                .map(|after| after.recorded_at.to_sqlx()),
        )
        .bind(request.after.as_ref().map(|after| after.work_id.as_str()))
        .bind(request.after.as_ref().map(|after| after.work_type))
        .bind(i64::from(request.limit) + 1)
}

/// One activity's individual charges, using the existing work/sequence key.
pub(super) fn charges<'q, O, S>(
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
    sqlx::query_as(wrap(include_str!("sql/charges.sql")))
        .bind(tenant.as_str())
        .bind(work.work_type())
        .bind(work.id().as_str())
        .bind(request.window.from.to_sqlx())
        .bind(request.window.through.to_sqlx())
        .bind(request.filter.model.as_deref())
        .bind(after)
        .bind(i64::from(request.limit) + 1)
        .bind(request.filter.fleet.as_ref().map(Uuid7::as_str))
        .bind(request.filter.work_type)
}
