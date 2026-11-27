//! Replay checks follow current authorization and precede lifecycle mutation.

mod sql;
#[cfg(test)]
mod tests;

use crate::{
    Result,
    access::Person,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
};
use af_core::id::Uuid7;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::PgConnection;
use std::time::Duration;

const CONTEXT: &str = "check fleet create replay";
const HASH_PREFIX: &str = "setup:v1:sha256:";

pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    let value = serde_json::to_value(value).map_err(stored)?;
    let bytes = serde_json::to_vec(&sorted(value)).map_err(stored)?;
    Ok(format!(
        "{HASH_PREFIX}{}",
        hex::encode(af_crypto::digest::sha256(&bytes))
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
        scalar @ (Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)) => scalar,
    }
}

/// The digest a receipt stores: the request's, bound to the person who sent
/// it, so another person reusing the key reads as a changed body.
pub(crate) fn bound(actor: &Uuid7, digest: &str) -> Result<String> {
    self::digest(&(actor.as_str(), digest))
}

/// [`sql::READ`]'s row, by column name.
#[derive(Deserialize, sqlx::FromRow)]
pub(crate) struct Receipt {
    request_digest: String,
    resource_id: Uuid7,
    /// Whether the named resource still exists.
    live: bool,
}

/// The fleet this operation already created inside `window`, when it
/// created one.
///
/// A receipt outlives the resource it names, so a retry after deletion answers
/// Gone here, in the same statement that reads the receipt. A receipt past the
/// window reads as no receipt, so the key is new.
///
/// # Errors
/// [`ErrorKind::Replay`] for another actor or a changed body, checked first;
/// [`ErrorKind::Gone`] when the named resource was deleted.
pub(crate) async fn find(
    connection: &mut PgConnection,
    scope: &str,
    operation: &Uuid7,
    actor: &Uuid7,
    digest: &str,
    window: Duration,
) -> Result<Option<Uuid7>> {
    let receipt = sqlx::query_as::<_, Receipt>(sql::READ)
        .bind(scope)
        .bind(operation)
        .bind(window)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    decide(receipt, actor, digest)
}

/// What a read receipt answers for `actor` sending `digest`.
pub(crate) fn decide(
    receipt: Option<Receipt>,
    actor: &Uuid7,
    digest: &str,
) -> Result<Option<Uuid7>> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_digest != bound(actor, digest)? {
        return Err(ErrorKind::Replay.into());
    }
    if !receipt.live {
        return Err(ErrorKind::Gone.into());
    }
    Ok(Some(receipt.resource_id))
}

pub(crate) struct Reservation<'a> {
    pub(crate) person: &'a Person,
    pub(crate) scope: &'a str,
    pub(crate) operation: &'a Uuid7,
    pub(crate) digest: &'a str,
    pub(crate) resource: &'a Uuid7,
    /// How long the receipt answers a repeat.
    pub(crate) window: Duration,
}

/// Files the operation's receipt; `false` when another claim filed it first.
///
/// A receipt past the replay window still holds the unique key, so it is
/// deleted first, on the caller's transaction. A concurrent claim on the same
/// expired key waits on that delete's row lock, finds the row gone, then loses
/// the insert to the new receipt and replays it: one new receipt either way.
pub(crate) async fn reserve(
    connection: &mut PgConnection,
    reservation: Reservation<'_>,
) -> Result<bool> {
    sqlx::query(sql::EXPIRE)
        .bind(reservation.scope)
        .bind(reservation.operation)
        .bind(reservation.window)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let inserted = sqlx::query(sql::INSERT)
        .bind(reservation.scope)
        .bind(reservation.operation)
        .bind(bound(&reservation.person.id, reservation.digest)?)
        .bind(reservation.resource)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(inserted.rows_affected() == 1)
}
