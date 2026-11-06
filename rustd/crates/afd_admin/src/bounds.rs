//! Transaction-local budgets for catalogue readers and serialized writers.
use crate::error::{Result, query};
use sqlx::PgConnection;
const LOCK_TIMEOUT: &str = "2000ms";
const STATEMENT_TIMEOUT: &str = "5000ms";
pub(crate) async fn apply(connection: &mut PgConnection, context: &'static str) -> Result<()> {
    sqlx::query(include_str!("sql/catalogue_bounds.sql"))
        .bind(LOCK_TIMEOUT)
        .bind(STATEMENT_TIMEOUT)
        .execute(connection)
        .await
        .map_err(query(context))?;
    Ok(())
}
