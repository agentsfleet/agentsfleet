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
use afd_core::{clock::UnixMillis, id::Uuid7};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use sqlx::{PgConnection, Row as _};

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

pub(crate) async fn find(
    connection: &mut PgConnection,
    scope: &str,
    operation: &Uuid7,
    actor: &Uuid7,
    digest: &str,
) -> Result<Option<Uuid7>> {
    sqlx::query(sql::EXPIRE)
        .bind(scope)
        .bind(operation.as_str())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = sqlx::query(sql::READ)
        .bind(scope)
        .bind(operation.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
    else {
        return Ok(None);
    };
    let accepted_actor: String = row.try_get(0).map_err(query(CONTEXT))?;
    let accepted_digest: String = row.try_get(1).map_err(query(CONTEXT))?;
    if accepted_actor != actor.as_str() || accepted_digest != digest {
        return Err(ErrorKind::Replay.into());
    }
    let purged: Option<i64> = row.try_get(3).map_err(query(CONTEXT))?;
    if purged.is_some() {
        return Err(ErrorKind::Gone.into());
    }
    let resource: String = row.try_get(2).map_err(query(CONTEXT))?;
    Ok(Some(Uuid7::parse(&resource)?))
}

pub(crate) struct Reservation<'a> {
    pub(crate) receipt_id: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) person: &'a Person,
    pub(crate) scope: &'a str,
    pub(crate) operation: &'a Uuid7,
    pub(crate) digest: &'a str,
    pub(crate) resource: &'a Uuid7,
    pub(crate) now: UnixMillis,
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
        .bind(reservation.now.as_millis())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(inserted.rows_affected() == 1)
}

pub(crate) async fn purge(connection: &mut PgConnection, resource: &Uuid7) -> Result<()> {
    sqlx::query(sql::PURGE)
        .bind(resource.as_str())
        .bind(afd_wire::authoring::REPLAY_TOMBSTONE_MS)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
