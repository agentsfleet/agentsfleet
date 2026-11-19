//! Plugin Inspect derives live readiness in the requesting workspace.

use super::{CONTEXT, Entry, EntryRow, Revision, RevisionRow, sql};
use crate::{
    Access, Result, Setup,
    access::{self, Person},
    error::{ErrorKind, raise::query},
    review,
    types::{Plugin, SourcePin},
};
use af_core::{clock, id::Uuid7};
use sqlx::{Connection as _, PgConnection};

impl Setup {
    /// Reads a visible entry and an explicitly pinned revision when supplied.
    ///
    /// # Errors
    /// Hides foreign entries/revisions and preserves datastore failures.
    pub async fn plugin(
        &self,
        access: Access<'_>,
        plugin: &Uuid7,
        selected: Option<&Uuid7>,
    ) -> Result<Plugin> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = entry(&mut tx, &person.owner, plugin, false).await?;
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
            &context(access, &person, &entry.id, &selected),
            &selected.candidate(),
            clock::now(),
        )
        .await?;
        let result = Plugin {
            card,
            head: selected.project(entry.id.clone()),
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
    plugin: &Uuid7,
    must_own: bool,
) -> Result<Entry> {
    let row: Option<EntryRow> = sqlx::query_as(if must_own { sql::OWNED } else { sql::ENTRY })
        .bind(plugin)
        .bind(owner)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::Hidden)?.decode()
}

pub(super) async fn revision(
    connection: &mut PgConnection,
    plugin: &Uuid7,
    id: &Uuid7,
) -> Result<Revision> {
    let row: Option<RevisionRow> = sqlx::query_as(sql::REVISION)
        .bind(plugin)
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::Hidden)?.decode()
}

pub(crate) fn context(
    access: Access<'_>,
    person: &Person,
    plugin: &Uuid7,
    revision: &Revision,
) -> review::Context {
    review::Context {
        subject: review::Subject::Plugin {
            plugin_id: plugin.to_owned(),
            revision_id: revision.id.clone(),
        },
        workspace_id: access.workspace.clone(),
        tenant_id: person.owner.clone(),
        creator_id: Some(person.id.clone()),
        instance_name: None,
        linked_fleet_id: None,
        source_pin: Some(SourcePin::Plugin {
            plugin_id: plugin.to_owned(),
            revision_id: revision.id.clone(),
        }),
    }
}
