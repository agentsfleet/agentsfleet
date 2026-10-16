//! Scope, version and live ownership are proved while both row locks hold.

use super::{EditRequest, sql};
use crate::{
    DesiredStatus, Refused, Result, Schedule,
    error::{self, ErrorKind},
};
use sqlx::{FromRow as _, PgConnection};

const CONTEXT: &str = "lock a schedule edit";

pub(super) async fn lock(
    connection: &mut PgConnection,
    request: &EditRequest<'_>,
) -> Result<Schedule> {
    let owned: Option<String> = sqlx::query_scalar(sql::LOCK)
        .bind(request.fleet.as_str())
        .bind(request.workspace.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if owned.is_none() {
        return Err(ErrorKind::Refused {
            refusal: Refused::NoSuchFleet,
        }
        .into());
    }
    let row = sqlx::query(crate::sql::LOCK_ONE)
        .bind(request.schedule.as_str())
        .bind(request.fleet.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?
        .ok_or(ErrorKind::Refused {
            refusal: Refused::NoSuchFleet,
        })?;
    let row = Schedule::from_row(&row).map_err(error::query(CONTEXT))?;
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
        .bind(request.schedule.as_str())
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
