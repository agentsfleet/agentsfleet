//! Creator privacy is stronger than ordinary workspace and platform scopes.

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    sql,
};
use af_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT: &str = "check current setup membership";

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

/// [`sql::access::PERSON`]'s and [`sql::access::PERSON_READ`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct PersonRow {
    user_id: Uuid7,
    owner_id: Uuid7,
    personal_owner: bool,
}

/// The authenticated person, still a member, with their row key-shared until
/// the caller's write commits.
pub(crate) async fn current(connection: &mut PgConnection, access: Access<'_>) -> Result<Person> {
    person(connection, access, sql::access::PERSON).await
}

/// [`current`] for a read or an assignment, which holds nothing: the same
/// membership test, with no lock on the person.
pub(crate) async fn unlocked(connection: &mut PgConnection, access: Access<'_>) -> Result<Person> {
    person(connection, access, sql::access::PERSON_READ).await
}

async fn person(
    connection: &mut PgConnection,
    access: Access<'_>,
    statement: &'static str,
) -> Result<Person> {
    let row: PersonRow = sqlx::query_as(statement)
        .bind(access.workspace)
        .bind(access.subject)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(ErrorKind::Hidden)?;
    if !row.personal_owner {
        let membership: Option<Uuid7> = sqlx::query_scalar(sql::access::MEMBER)
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
        id: row.user_id.clone(),
        owner: row.owner_id.clone(),
    })
}
