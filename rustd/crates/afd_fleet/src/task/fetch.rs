//! Bound connected reads as well as pool acquisition; preserve healthy pooling.

use std::time::Duration;

use afd_db::Db;
use sqlx::{
    Connection as _, FromRow, Postgres,
    postgres::{PgArguments, PgRow},
    query::QueryAs,
};
use tokio::time::{Instant, timeout_at};

use super::{TaskRefusal, TaskScope};
use crate::error::{Error, ErrorKind, Result, query};

/// Includes acquisition, transaction setup, rows and commit.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Server cancellation frees lock waits even when a client stops receiving.
const STATEMENT_TIMEOUT: &str = "3000ms";

pub(super) async fn rows<R>(
    database: &Db,
    statement: QueryAs<'_, Postgres, R, PgArguments>,
    fleet: Option<&TaskScope>,
    context: &'static str,
) -> Result<Vec<R>>
where
    R: for<'row> FromRow<'row, PgRow> + Send + Unpin,
{
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut connection = timeout_at(deadline, database.acquire()).await??;
    let result = timeout_at(deadline, async {
        let mut transaction = connection.begin().await.map_err(query(context))?;
        sqlx::query(include_str!("sql/read_timeout.sql"))
            .bind(STATEMENT_TIMEOUT)
            .execute(&mut *transaction)
            .await
            .map_err(query(context))?;
        if let Some(scope) = fleet {
            let owned: bool = sqlx::query_scalar(include_str!("sql/read_scope.sql"))
                .bind(scope.fleet_id.as_str())
                .bind(scope.workspace_id.as_str())
                .fetch_one(&mut *transaction)
                .await
                .map_err(failure(context))?;
            if !owned {
                return Err(TaskRefusal::NotFound.into());
            }
        }
        let rows = statement
            .fetch_all(&mut *transaction)
            .await
            .map_err(failure(context))?;
        transaction.commit().await.map_err(query(context))?;
        Ok(rows)
    })
    .await;
    match result {
        Ok(answer) => answer,
        Err(source) => {
            // The wire state is unknown. Drop the raw socket and release its
            // pool permit instead of returning a busy connection for reuse.
            drop(connection.detach());
            Err(source.into())
        }
    }
}

fn failure(context: &'static str) -> impl FnOnce(sqlx::Error) -> Error {
    move |source| {
        if source
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref()
            == Some("57014")
        {
            Error::from(ErrorKind::TaskReadCancelled { source })
        } else {
            query(context)(source)
        }
    }
}
