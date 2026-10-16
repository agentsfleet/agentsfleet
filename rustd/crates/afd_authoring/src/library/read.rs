//! Library Inspect derives live readiness in the requesting workspace.

use super::{CONTEXT, Entry, PLATFORM, Revision, TENANT, sql};
use crate::{
    Access, Result, Setup,
    access::{self, Person},
    draft,
    error::{ErrorKind, raise::query},
    review,
    types::{Library, SourcePin},
};
use afd_core::{clock, id::Uuid7};
use sqlx::{Connection as _, PgConnection};

impl Setup {
    /// Reads a visible entry and an explicitly pinned revision when supplied.
    ///
    /// # Errors
    /// Hides foreign entries/revisions and preserves datastore failures.
    pub async fn library(
        &self,
        access: Access<'_>,
        library: &str,
        selected: Option<&Uuid7>,
    ) -> Result<Library> {
        valid_id(library)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = entry(&mut tx, &person.owner, library, false).await?;
        let head = revision(&mut tx, &entry.id, &entry.head_revision_id).await?;
        let card = entry.card(&head);
        let selected = match selected {
            Some(selected) if selected != &entry.head_revision_id => {
                revision(&mut tx, &entry.id, selected).await?
            }
            _ => head,
        };
        let review = review::build(
            self,
            &mut tx,
            &context(access, &person, &entry.public_id(), &selected),
            &selected.candidate(),
            clock::now().as_millis(),
        )
        .await?;
        let result = Library {
            card,
            head: selected.project(entry.public_id()),
            review,
            etag: entry.etag(),
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(result)
    }
}

pub(super) async fn entry(
    connection: &mut PgConnection,
    owner: &Uuid7,
    library: &str,
    must_own: bool,
) -> Result<Entry> {
    valid_id(library)?;
    let row: Option<String> = sqlx::query_scalar(if must_own { sql::OWNED } else { sql::ENTRY })
        .bind(library)
        .bind(owner.as_str())
        .bind(if must_own { TENANT } else { PLATFORM })
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    draft::value(&row.ok_or(ErrorKind::Hidden)?)
}

pub(super) async fn revision(
    connection: &mut PgConnection,
    library: &Uuid7,
    id: &Uuid7,
) -> Result<Revision> {
    let row: Option<String> = sqlx::query_scalar(sql::REVISION)
        .bind(library.as_str())
        .bind(id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    draft::value(&row.ok_or(ErrorKind::Hidden)?)
}

pub(crate) fn context(
    access: Access<'_>,
    person: &Person,
    library: &str,
    revision: &Revision,
) -> review::Context {
    review::Context {
        subject: review::Subject::Library {
            library_id: library.to_owned(),
            revision_id: revision.id.clone(),
        },
        workspace_id: access.workspace.clone(),
        tenant_id: person.owner.clone(),
        creator_id: Some(person.id.clone()),
        instance_name: None,
        linked_fleet_id: None,
        source_pin: Some(SourcePin::Library {
            library_id: library.to_owned(),
            revision_id: revision.id.clone(),
        }),
    }
}

#[derive(garde::Validate)]
struct Identifier<'a> {
    #[garde(
        length(bytes, min = 1, max = 128),
        custom(afd_validate::rules::nul_free)
    )]
    value: &'a str,
}

pub(super) fn valid_id(value: &str) -> Result<()> {
    use garde::Validate as _;
    Identifier { value }.validate()?;
    Ok(())
}
