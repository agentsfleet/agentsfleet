//! Replay checks follow current authorization and precede lifecycle mutation.

mod sql;

use crate::{
    Result,
    access::Person,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
};
use afd_core::id::Uuid7;
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;
use std::time::Duration;

const CONTEXT: &str = "check authoring operation replay";
const HASH_PREFIX: &str = "authoring:v1:sha256:";

/// How long a receipt answers its key, measured from its `created_at`.
///
/// Inside it the same key and body replay the first result; at and past it the
/// key is new and the receipt is swept.
const REPLAY_WINDOW: Duration = afd_core::timing::DAY;

/// The most receipts one maintenance pass deletes.
const SWEEP_LIMIT: i64 = 64;

pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    let value = serde_json::to_value(value).map_err(stored)?;
    let bytes = serde_json::to_vec(&sorted(value)).map_err(stored)?;
    Ok(format!(
        "{HASH_PREFIX}{}",
        hex::encode(Sha256::digest(bytes))
    ))
}

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        scalar => scalar,
    }
}

/// [`sql::READ`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Receipt {
    user_id: String,
    request_digest: String,
    resource_id: String,
}

/// The resource this operation already created inside [`REPLAY_WINDOW`], when
/// it created one.
///
/// A receipt names its resource even after that resource was deleted; the
/// caller that resolves it answers a missing resource as Gone. A receipt past
/// the window reads as no receipt, so the key is new.
pub(crate) async fn find(
    connection: &mut PgConnection,
    scope: &str,
    operation: &Uuid7,
    actor: &Uuid7,
    digest: &str,
) -> Result<Option<Uuid7>> {
    let Some(receipt) = sqlx::query_as::<_, Receipt>(sql::READ)
        .bind(scope)
        .bind(operation.as_str())
        .bind(REPLAY_WINDOW)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
    else {
        return Ok(None);
    };
    if receipt.user_id != actor.as_str() || receipt.request_digest != digest {
        return Err(ErrorKind::Replay.into());
    }
    Ok(Some(Uuid7::parse(&receipt.resource_id)?))
}

pub(crate) struct Reservation<'a> {
    pub(crate) receipt_id: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) person: &'a Person,
    pub(crate) scope: &'a str,
    pub(crate) operation: &'a Uuid7,
    pub(crate) digest: &'a str,
    pub(crate) resource: &'a Uuid7,
}

/// Files the operation's receipt; `false` when another claim filed it first.
///
/// A receipt past [`REPLAY_WINDOW`] still holds the unique key, so it is
/// deleted first, on the caller's transaction. A concurrent claim on the same
/// expired key waits on that delete's row lock, finds the row gone, then loses
/// the insert to the new receipt and replays it: one new receipt either way.
pub(crate) async fn reserve(
    connection: &mut PgConnection,
    reservation: Reservation<'_>,
) -> Result<bool> {
    sqlx::query(sql::EXPIRE)
        .bind(reservation.scope)
        .bind(reservation.operation.as_str())
        .bind(REPLAY_WINDOW)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let inserted = sqlx::query(sql::INSERT)
        .bind(reservation.receipt_id.as_str())
        .bind(reservation.workspace.as_str())
        .bind(reservation.person.id.as_str())
        .bind(reservation.scope)
        .bind(reservation.operation.as_str())
        .bind(reservation.digest)
        .bind(reservation.resource.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(inserted.rows_affected() == 1)
}

/// Deletes one bounded batch of receipts past [`REPLAY_WINDOW`], returning how
/// many went.
///
/// Rows a claim holds are skipped; that claim deletes its own expired row.
pub(crate) async fn sweep(connection: &mut PgConnection) -> Result<u64> {
    let swept = sqlx::query(sql::SWEEP)
        .bind(REPLAY_WINDOW)
        .bind(SWEEP_LIMIT)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(swept.rows_affected())
}
