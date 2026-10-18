//! Replay receipts and candidate locks make publication one explicit mutation.

mod create;
mod persist;
mod platform;
mod revision;

use super::{CONTEXT, Entry, read, sql};
use crate::{
    Result,
    access::Person,
    error::{ErrorKind, raise::query},
    replay,
    types::Published,
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

async fn accepted(
    connection: &mut PgConnection,
    owner: &Uuid7,
    revision: &Uuid7,
) -> Result<Published> {
    let library: Option<String> = sqlx::query_scalar(sql::ACCEPTED)
        .bind(revision.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let library = library.ok_or(ErrorKind::Gone)?;
    let entry = read::entry(connection, owner, &library, false).await?;
    Ok(project(&entry, revision))
}

fn project(entry: &Entry, revision: &Uuid7) -> Published {
    Published {
        library_id: entry.public_id(),
        revision_id: revision.clone(),
        head_revision_id: entry.head_revision_id.clone(),
        etag: entry.etag(),
    }
}

/// One publication request's replay identity: who asked, which scope the
/// receipt is filed under, the client's operation and the body's digest.
///
/// Both publication verbs read the receipt back three times — before the
/// candidate is captured, after it is locked, and after losing the reservation
/// race — so the four values travel together rather than as six arguments at
/// every lookup.
struct Replay<'a> {
    person: &'a Person,
    scope: &'a str,
    operation: &'a Uuid7,
    digest: &'a str,
}

impl Replay<'_> {
    /// The publication this request already made, if it made one.
    async fn existing(&self, connection: &mut PgConnection) -> Result<Option<Published>> {
        let found = replay::find(
            connection,
            self.scope,
            self.operation,
            &self.person.id,
            self.digest,
        )
        .await?;
        match found {
            Some(revision) => accepted(connection, &self.person.owner, &revision)
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    /// The publication a concurrent request reserved first. `Gone` when its
    /// receipt no longer reads back.
    async fn replayed(&self, connection: &mut PgConnection) -> Result<Published> {
        Ok(self.existing(connection).await?.ok_or(ErrorKind::Gone)?)
    }

    /// The receipt this request files for `resource` before writing it.
    fn reservation<'r>(
        &'r self,
        receipt_id: &'r Uuid7,
        workspace: &'r Uuid7,
        resource: &'r Uuid7,
    ) -> replay::Reservation<'r> {
        replay::Reservation {
            receipt_id,
            workspace,
            person: self.person,
            scope: self.scope,
            operation: self.operation,
            digest: self.digest,
            resource,
        }
    }
}
