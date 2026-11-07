//! Creator privacy is stronger than ordinary workspace and platform scopes.

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    sql,
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT: &str = "check current authoring membership";

/// Request identity supplied by the already-authenticated HTTP adapter.
#[derive(Debug, Clone, Copy)]
pub struct Access<'a> {
    /// Workspace in the route, already checked by ordinary ownership middleware.
    pub workspace: &'a Uuid7,
    /// Authenticated person; never accepted from a JSON body.
    pub subject: &'a str,
}

pub(crate) struct Person {
    pub(crate) id: Uuid7,
    pub(crate) owner: Uuid7,
}

/// [`sql::access::PERSON`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct PersonRow {
    user_id: String,
    owner_id: String,
    personal_owner: bool,
}

pub(crate) async fn current(connection: &mut PgConnection, access: Access<'_>) -> Result<Person> {
    let row: PersonRow = sqlx::query_as(sql::access::PERSON)
        .bind(access.workspace.as_str())
        .bind(access.subject)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(ErrorKind::Hidden)?;
    if !row.personal_owner {
        let membership: Option<String> = sqlx::query_scalar(sql::access::MEMBER)
            .bind(&row.owner_id)
            .bind(&row.user_id)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?;
        if membership.is_none() {
            return Err(ErrorKind::Hidden.into());
        }
    }
    Ok(Person {
        id: Uuid7::parse(&row.user_id)?,
        owner: Uuid7::parse(&row.owner_id)?,
    })
}
