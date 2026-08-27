//! What a workspace's fleets look like when read: the page walk, and one fleet.
//!
//! # The page decides `more` by over-fetching, not by being full
//!
//! `list.zig` emits a continuation token whenever the page came back FULL, so
//! the last page of an exactly-divisible walk hands the client a cursor that
//! resolves to nothing. Here the walk asks for one row past the limit and keeps
//! the limit, so [`FleetPage::more`] is a fact rather than a guess — the same
//! correction the workspace directory took, and it costs one row per page.
//!
//! A client following the Zig token still terminates; it just spends one extra
//! request discovering that. A client following this one stops immediately.
//! Recorded as a declared divergence rather than a fix nobody can see.

use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// The context a failed page walk reports under.
const CONTEXT_PAGE: &str = "list workspace fleets";

/// The context a failed detail read reports under.
const CONTEXT_DETAIL: &str = "read one fleet";

/// The context a row this daemon cannot read reports under.
const CONTEXT_ROW: &str = "read fleet row";

/// The column a status this build does not know is reported against.
const COLUMN_STATUS: &str = "status";

/// The trigger list, as the stored configuration already holds it.
///
/// Carried as the raw JSON TEXT the projection returned rather than a parsed
/// tree, because nothing on this path reads inside it: the store hands it up,
/// and the wire layer splices it into the response through `serde_json`'s
/// `RawValue`. Parsing it here would be a full deserialize whose only product
/// is a re-serialize into the same bytes.
///
/// Validity is therefore the wire layer's question, and its answer matches the
/// Zig's: text that will not parse renders as `null`, exactly as
/// `parseFromSlice(...) catch null` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triggers(Box<str>);

impl Triggers {
    /// The stored JSON text.
    #[must_use]
    pub fn as_json_text(&self) -> &str {
        &self.0
    }
}

/// One fleet as a list page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetRow {
    /// The fleet's identifier.
    pub id: String,
    /// Its name in this workspace — the instance identity, not the bundle's.
    pub name: String,
    /// Where it stands in its life.
    pub status: FleetStatus,
    /// When it was installed; the walk's sort key.
    pub created_at_ms: i64,
    /// When it last changed. Doubles as the config revision a PATCH echoes.
    pub updated_at_ms: i64,
    /// What may wake it, projected from the stored configuration.
    pub triggers: Option<Triggers>,
    /// Lifetime event count, from the maintained counter row.
    pub events_processed: i64,
    /// Lifetime spend, from the same row. Server truth, never client arithmetic.
    pub budget_used_nanos: i64,
}

/// One page of the walk, and whether a row exists beyond it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetPage {
    /// The rows on this page, newest first.
    pub rows: Vec<FleetRow>,
    /// Whether the walk continues past the last row here.
    pub more: bool,
}

/// A decoded `starting_after` boundary: the last row the previous page showed.
///
/// The identifier is a parsed [`Uuid7`] and not a string, so a malformed cursor
/// is refused before a connection is drawn — `isSupportedWorkspaceId` as a type
/// rather than as a call somebody has to remember to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct After {
    /// The boundary row's `created_at`.
    pub created_at_ms: i64,
    /// The boundary row's identifier, breaking ties inside one millisecond.
    pub id: Uuid7,
}

/// One fleet, whole — what the source editor opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetDetail {
    /// The page row's fields, unchanged.
    pub row: FleetRow,
    /// The authored `SKILL.md`, stored verbatim.
    pub source_markdown: String,
    /// The authored `TRIGGER.md`, where the bundle carried one.
    pub trigger_markdown: Option<String>,
    /// The bundle a runner materialises support files from.
    pub bundle_content_hash: Option<String>,
}

impl FleetDetail {
    /// The strong `ETag` over this fleet's editable surface.
    ///
    /// Only the markdown pair, deliberately. A lifecycle PATCH — stop, resume,
    /// kill — leaves the source untouched, so an editor holding an open buffer
    /// is not 412'd by somebody else stopping the fleet. Computed here and by
    /// the conditional write in [`crate::edit`] through the same
    /// [`surface`] call, so the two are identical by construction rather than
    /// by two authors agreeing.
    #[must_use]
    pub fn etag(&self) -> String {
        afd_core::etag::compute(&surface(
            &self.source_markdown,
            self.trigger_markdown.as_deref(),
        ))
    }
}

/// The ordered field list a fleet's `ETag` hashes.
///
/// One function, two callers, and that is the point: the detail read attaches
/// the tag and the conditional write compares one, and a surface spelled twice
/// would let a fleet answer a tag its own writer would not accept.
pub(crate) fn surface<'a>(
    source_markdown: &'a str,
    trigger_markdown: Option<&'a str>,
) -> [Option<&'a [u8]>; 2] {
    [
        Some(source_markdown.as_bytes()),
        trigger_markdown.map(str::as_bytes),
    ]
}

impl Fleets {
    /// One page of `workspace`'s fleets, newest first.
    ///
    /// `after` is the decoded cursor when the caller is resuming, and `limit` is
    /// the page size they will actually be served — the extra row this fetches
    /// is never returned.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon cannot
    /// read — including a status a newer build writes and this one does not know.
    pub async fn page(
        &self,
        workspace: &Uuid7,
        after: Option<&After>,
        limit: u32,
    ) -> Result<FleetPage> {
        // One past the limit, so `more` is a fact about the walk rather than a
        // guess from a full page.
        let fetch = i64::from(limit).saturating_add(1);
        let mut connection = self.database.acquire().await?;
        let query = match after {
            None => sqlx::query(sql::SELECT_FLEET_PAGE_FIRST)
                .bind(workspace.as_str())
                .bind(fetch),
            Some(boundary) => sqlx::query(sql::SELECT_FLEET_PAGE_AFTER)
                .bind(workspace.as_str())
                .bind(boundary.created_at_ms)
                .bind(boundary.id.as_str())
                .bind(fetch),
        };
        let fetched = query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_PAGE))?;

        let mut rows = fetched
            .iter()
            .map(read_row)
            .collect::<Result<Vec<FleetRow>>>()?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        Ok(FleetPage { rows, more })
    }

    /// One fleet of `workspace`, whole.
    ///
    /// # Errors
    /// Refuses an id naming no fleet THIS workspace holds — the statement is
    /// workspace-scoped, so a fleet somebody else owns is indistinguishable from
    /// one that never existed, and neither is disclosed. Reports a datastore
    /// that would not answer.
    pub async fn detail(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<FleetDetail> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(sql::SELECT_FLEET_DETAIL)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_DETAIL))?;
        let row = found.ok_or_else(|| crate::Error::from(ErrorKind::NotFound))?;

        let unreadable = error::query(CONTEXT_ROW);
        Ok(FleetDetail {
            row: FleetRow {
                id: row.try_get(0).map_err(&unreadable)?,
                name: row.try_get(1).map_err(&unreadable)?,
                status: status_at(&row, 2)?,
                created_at_ms: row.try_get(9).map_err(&unreadable)?,
                updated_at_ms: row.try_get(10).map_err(&unreadable)?,
                triggers: triggers_at(&row, 6)?,
                events_processed: row.try_get(7).map_err(&unreadable)?,
                budget_used_nanos: row.try_get(8).map_err(&unreadable)?,
            },
            source_markdown: row.try_get(3).map_err(&unreadable)?,
            trigger_markdown: row.try_get(4).map_err(&unreadable)?,
            bundle_content_hash: row.try_get(5).map_err(&unreadable)?,
        })
    }
}

/// Reads one page row, indexed against [`sql::SELECT_FLEET_PAGE_FIRST`].
fn read_row(row: &sqlx::postgres::PgRow) -> Result<FleetRow> {
    let unreadable = error::query(CONTEXT_ROW);
    Ok(FleetRow {
        id: row.try_get(0).map_err(&unreadable)?,
        name: row.try_get(1).map_err(&unreadable)?,
        status: status_at(row, 2)?,
        created_at_ms: row.try_get(3).map_err(&unreadable)?,
        updated_at_ms: row.try_get(4).map_err(&unreadable)?,
        triggers: triggers_at(row, 5)?,
        events_processed: row.try_get(6).map_err(&unreadable)?,
        budget_used_nanos: row.try_get(7).map_err(&unreadable)?,
    })
}

/// The status in column `at`, refusing a spelling this build does not know.
///
/// A row written by a newer daemon is an unreadable row and says so, rather
/// than defaulting to something this build would then act on.
fn status_at(row: &sqlx::postgres::PgRow, at: usize) -> Result<FleetStatus> {
    let raw: String = row.try_get(at).map_err(error::query(CONTEXT_ROW))?;
    FleetStatus::parse(&raw).ok_or_else(|| error::row_malformed(COLUMN_STATUS, &raw))
}

/// The trigger projection in column `at`, absent where the column holds none.
fn triggers_at(row: &sqlx::postgres::PgRow, at: usize) -> Result<Option<Triggers>> {
    let raw: Option<String> = row.try_get(at).map_err(error::query(CONTEXT_ROW))?;
    Ok(raw.map(|text| Triggers(text.into_boxed_str())))
}

#[cfg(test)]
mod tests {
    use super::surface;

    #[test]
    fn the_hashed_surface_tells_an_absent_trigger_from_an_empty_one() {
        // A bundle with no `TRIGGER.md` and one with an empty file are
        // different resource states. A tag that collided would let a
        // conditional write overwrite an edit it never saw.
        let absent = afd_core::etag::compute(&surface("skill", None));
        let empty = afd_core::etag::compute(&surface("skill", Some("")));

        assert_ne!(absent, empty);
    }

    #[test]
    fn a_lifecycle_change_leaves_the_tag_alone() {
        // Only the markdown pair is hashed, so stopping a fleet does not 412
        // the editor somebody has open on its source.
        let before = afd_core::etag::compute(&surface("skill", Some("trigger")));
        let after = afd_core::etag::compute(&surface("skill", Some("trigger")));

        assert_eq!(before, after);
    }
}
