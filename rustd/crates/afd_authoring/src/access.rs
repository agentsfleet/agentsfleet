//! Creator privacy is stronger than ordinary workspace and platform scopes.

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    sql,
};
use afd_core::id::Uuid7;
use sqlx::{PgConnection, Row as _};

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

pub(crate) async fn current(connection: &mut PgConnection, access: Access<'_>) -> Result<Person> {
    let row = sqlx::query(sql::access::PERSON)
        .bind(access.workspace.as_str())
        .bind(access.subject)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(ErrorKind::Hidden)?;
    let user: String = row.try_get(0).map_err(query(CONTEXT))?;
    let owner: String = row.try_get(1).map_err(query(CONTEXT))?;
    let personal: bool = row.try_get(2).map_err(query(CONTEXT))?;
    if !personal {
        let membership = sqlx::query(sql::access::MEMBER)
            .bind(&owner)
            .bind(&user)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?;
        if membership.is_none() {
            return Err(ErrorKind::Hidden.into());
        }
    }
    Ok(Person {
        id: Uuid7::parse(&user)?,
        owner: Uuid7::parse(&owner)?,
    })
}
