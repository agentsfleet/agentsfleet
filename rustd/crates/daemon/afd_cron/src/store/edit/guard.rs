//! Scope, version and live ownership are proved while both row locks hold.

use super::{EditRequest, sql};
use crate::{
    DesiredStatus, Refused, Result, Schedule,
    error::{self, ErrorKind},
};
use af_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT: &str = "lock a schedule edit";

pub(super) async fn lock(
    connection: &mut PgConnection,
    request: &EditRequest<'_>,
) -> Result<Schedule> {
    let owned: Option<Uuid7> = sqlx::query_scalar(crate::sql::LOCK_FLEET_IN_WORKSPACE)
        .bind(request.fleet)
        .bind(request.workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if owned.is_none() {
        return Err(ErrorKind::Refused {
            refusal: Refused::NoSuchFleet,
        }
        .into());
    }
    let row = sqlx::query_as::<_, Schedule>(crate::sql::LOCK_ONE)
        .bind(request.schedule)
        .bind(request.fleet)
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?
        .ok_or(ErrorKind::Refused {
            refusal: Refused::NoSuchFleet,
        })?;
    check_tag(&row, request.expected)?;
    available(connection, request, &row).await?;
    Ok(row)
}

async fn available(
    connection: &mut PgConnection,
    request: &EditRequest<'_>,
    row: &Schedule,
) -> Result<()> {
    let held: bool = sqlx::query_scalar(sql::HELD)
        .bind(request.schedule)
        .fetch_one(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if held {
        return Err(ErrorKind::Held.into());
    }
    if row.desired_status == DesiredStatus::Deleting {
        return Err(ErrorKind::Deleting.into());
    }
    Ok(())
}

fn check_tag(row: &Schedule, expected: Option<&str>) -> Result<()> {
    let expected = expected.ok_or(ErrorKind::Precondition)?;
    let etag = row.etag();
    if expected != etag {
        return Err(ErrorKind::Stale { etag }.into());
    }
    Ok(())
}
