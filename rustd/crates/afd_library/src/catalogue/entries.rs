//! `GET`/`DELETE /v1/workspaces/{workspace_id}/library-entries` — what one
//! workspace onboarded, and taking one of them back out.
//!
//! # A second collection, not a filter on the gallery
//!
//! The merged gallery ([`super::gallery`]) answers what a workspace can
//! INSTALL: the published platform catalogue unioned with the workspace's own
//! rows. This answers what a workspace OWNS, which is the tenant half alone.
//! They are different questions with different shapes, and the models domain
//! settled the same split before this one existed — `/v1/models` is the
//! catalogue an operator installs from, `/v1/tenants/me/models` is the registry
//! they administer, and no query parameter selects between them. Adding a tier
//! filter to the gallery instead would make one endpoint answer two shapes and
//! leave the cursor resolving against a sequence that changes with the filter.
//!
//! # Removal is permanent, and nothing downstream notices
//!
//! There is no soft-delete column, no marker row and no visibility flip. A
//! fleet installed from an entry copied `skill_markdown`, `trigger_markdown`
//! and `content_hash` out of it at install time and carries its own; no foreign
//! key points here from the fleet side. Removing an entry cannot disturb a
//! fleet running from it, which is what licenses the hard delete — and what
//! `integration_fleet_lifecycle` asserts rather than assumes.

mod sql;

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::Libraries;
use crate::Result;
use crate::error::database;

/// The context a failed owned-collection read reports under.
const CONTEXT_OWNED: &str = "list a workspace's own fleet-library entries";

/// The context a failed removal reports under.
const CONTEXT_REMOVE: &str = "remove a workspace's fleet-library entry";

/// Where a later page of the owned collection resumes.
///
/// Two columns, because one table has one order. The gallery's boundary
/// carries a tier as well, since it resumes a walk across two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPosition {
    /// The onboarding instant of the last entry served.
    pub created_at_ms: i64,
    /// That entry's identifier, breaking ties at the same instant.
    pub id: String,
}

/// One entry a workspace onboarded.
///
/// Carries no bundle content — the statements project none. `content_hash` is
/// here because it is what distinguishes two near-identical onboardings from
/// each other, which is the question this page exists to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedEntry {
    /// The entry's identifier, which is what a removal names.
    pub id: String,
    /// The bundle's declared name.
    pub name: String,
    /// The bundle's declared description.
    pub description: String,
    /// Where the bundle came from — an upload, a repository, a template.
    pub source_kind: String,
    /// The reference within that source.
    pub source_ref: String,
    /// The bundle's content hash, the domain key's other half.
    pub content_hash: String,
    /// When the workspace onboarded it.
    pub created_at_ms: i64,
}

/// One page of a workspace's own entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedPage {
    /// The entries on this page, newest first.
    pub items: Vec<OwnedEntry>,
    /// Where the next page resumes, or `None` at the end of the walk.
    pub next: Option<EntryPosition>,
}

impl Libraries {
    /// One page of the entries `workspace` onboarded.
    ///
    /// `after` is the decoded boundary from the caller's cursor, already
    /// checked against the workspace in the path. The read is scoped to the
    /// PATH's workspace, never the cursor's.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon cannot
    /// read.
    pub async fn owned_entries(
        &self,
        workspace: &Uuid7,
        limit: u32,
        after: Option<&EntryPosition>,
    ) -> Result<OwnedPage> {
        let fetch = i64::from(limit) + 1;
        let statement = match after {
            None => sqlx::query(sql::FIRST_PAGE)
                .bind(workspace.as_str())
                .bind(fetch),
            Some(position) => sqlx::query(sql::PAGE_AFTER)
                .bind(workspace.as_str())
                .bind(position.created_at_ms)
                .bind(position.id.as_str())
                .bind(fetch),
        };

        let mut connection = self.0.acquire().await?;
        let rows = statement
            .fetch_all(&mut *connection)
            .await
            .map_err(database(CONTEXT_OWNED))?;

        let has_more = rows.len() > limit as usize;
        let items: Vec<OwnedEntry> = rows
            .iter()
            .take(limit as usize)
            .map(decode)
            .collect::<Result<_>>()?;

        // From the last SERVED entry, never the over-fetched one: the seek is
        // strict, so the next page resumes after what the caller saw.
        let next = has_more.then(|| items.last().map(position_of)).flatten();
        Ok(OwnedPage { items, next })
    }

    /// Removes one entry, if `workspace` is the workspace that onboarded it.
    ///
    /// Answers whether a row left. The caller does NOT turn that into two
    /// different responses: an identifier this workspace does not own and one
    /// already removed both answer `204`, because telling them apart needs an
    /// unscoped read whose only effect is to confirm the identifier exists
    /// somewhere. The boolean is what the scoped log line reports.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A row that was not there is
    /// not an error.
    pub async fn remove_entry(&self, workspace: &Uuid7, entry: &Uuid7) -> Result<bool> {
        let mut connection = self.0.acquire().await?;
        let removed = sqlx::query(sql::REMOVE_ENTRY)
            .bind(entry.as_str())
            .bind(workspace.as_str())
            .execute(&mut *connection)
            .await
            .map_err(database(CONTEXT_REMOVE))?;

        Ok(removed.rows_affected() > 0)
    }
}

/// Where a later page resumes after `entry`.
fn position_of(entry: &OwnedEntry) -> EntryPosition {
    EntryPosition {
        created_at_ms: entry.created_at_ms,
        id: entry.id.clone(),
    }
}

/// Reads one row into an owned entry.
///
/// Positional, matching every other read in this workspace: the projection and
/// this function are one definition, and reading by name would hide a
/// projection that had drifted out of order.
fn decode(row: &PgRow) -> Result<OwnedEntry> {
    let unreadable = database(CONTEXT_OWNED);
    Ok(OwnedEntry {
        id: row.try_get(0).map_err(&unreadable)?,
        name: row.try_get(1).map_err(&unreadable)?,
        description: row.try_get(2).map_err(&unreadable)?,
        source_kind: row.try_get(3).map_err(&unreadable)?,
        source_ref: row.try_get(4).map_err(&unreadable)?,
        content_hash: row.try_get(5).map_err(&unreadable)?,
        created_at_ms: row.try_get(6).map_err(&unreadable)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{EntryPosition, OwnedEntry, position_of};

    fn entry(id: &str, created_at_ms: i64) -> OwnedEntry {
        OwnedEntry {
            id: id.to_owned(),
            name: "github-pr-reviewer".to_owned(),
            description: "reviews pull requests".to_owned(),
            source_kind: "upload".to_owned(),
            source_ref: "bundle.tar.gz".to_owned(),
            content_hash: "cafe".to_owned(),
            created_at_ms,
        }
    }

    /// The boundary is the last SERVED entry, both columns of it.
    ///
    /// A cursor built from anything else resumes in the wrong place: from the
    /// over-fetched row it skips one entry per page, and from the identifier
    /// alone it cannot break a tie at the same instant.
    #[test]
    fn the_boundary_carries_both_columns_of_the_last_served_entry() {
        let last = entry("0199-zzz", 1_764_000_000_000);
        assert_eq!(
            position_of(&last),
            EntryPosition {
                created_at_ms: 1_764_000_000_000,
                id: "0199-zzz".to_owned(),
            }
        );
    }
}
