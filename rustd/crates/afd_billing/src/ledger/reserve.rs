//! [`LedgerBiller::reserve`](super::LedgerBiller): one hold per work, never
//! more in total than the locked balance.

use sqlx::PgConnection;

use super::{
    CONTEXT_RESERVE, LedgerBiller, SELECT_HELD_BY_OTHERS, SELECT_HOLD, UPSERT_HOLD, lock_wallet,
};
use crate::biller::{Hold, Reservation};
use crate::error::query;
use crate::{Nanos, Result};

/// A work's existing hold, by column name.
#[derive(Debug, sqlx::FromRow)]
struct HoldRow {
    ceiling_nanos: i64,
    captured_nanos: i64,
    released: bool,
}

/// Holds `ask.ceiling` for its work inside `connection`'s transaction.
///
/// Under the payer's wallet lock: a released work answers [`Hold::Closed`]; an
/// ask no higher than the existing ceiling answers the existing hold
/// unchanged; otherwise the hold is raised only if the balance, less every
/// OTHER open hold's uncaptured remainder, covers what this work has not yet
/// captured.
pub(super) async fn reserve(
    biller: &LedgerBiller,
    connection: &mut PgConnection,
    ask: &Reservation,
) -> Result<Hold> {
    let balance = lock_wallet(connection, &ask.payer.tenant, CONTEXT_RESERVE).await?;
    let work_type = ask.work.work_type();
    let work_id = ask.work.id().as_str();

    let existing: Option<HoldRow> = sqlx::query_as(SELECT_HOLD)
        .bind(work_type)
        .bind(work_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_RESERVE))?;
    let (ceiling, captured) = match existing {
        Some(row) if row.released => return Ok(Hold::Closed),
        Some(row) => (
            Nanos::from_i64(row.ceiling_nanos),
            Nanos::from_i64(row.captured_nanos),
        ),
        None => (Nanos::ZERO, Nanos::ZERO),
    };
    if ask.ceiling <= ceiling && ceiling > Nanos::ZERO {
        return Ok(Hold::Held { ceiling, captured });
    }

    let held_by_others: i64 = sqlx::query_scalar(SELECT_HELD_BY_OTHERS)
        .bind(ask.payer.tenant.as_str())
        .bind(work_type)
        .bind(work_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT_RESERVE))?;
    let available = Nanos::from_i64(balance.as_i64().saturating_sub(held_by_others).max(0));
    let needed = Nanos::from_i64(
        ask.ceiling
            .as_i64()
            .saturating_sub(captured.as_i64())
            .max(0),
    );
    if !available.covers(needed) {
        return Ok(Hold::Short {
            available,
            asked: ask.ceiling,
        });
    }

    sqlx::query(UPSERT_HOLD)
        .bind(biller.mint()?.as_str())
        .bind(work_type)
        .bind(work_id)
        .bind(ask.payer.tenant.as_str())
        .bind(ask.ceiling.as_i64())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_RESERVE))?;
    Ok(Hold::Held {
        ceiling: ceiling.max(ask.ceiling),
        captured,
    })
}
