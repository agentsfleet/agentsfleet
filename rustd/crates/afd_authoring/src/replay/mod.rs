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

const CONTEXT: &str = "check authoring operation replay";
const HASH_PREFIX: &str = "authoring:v1:sha256:";

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

/// The resource this operation already created, when it created one.
///
/// A receipt names its resource even after that resource was deleted; the
/// caller that resolves it answers a missing resource as Gone.
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

pub(crate) async fn reserve(
    connection: &mut PgConnection,
    reservation: Reservation<'_>,
) -> Result<bool> {
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
