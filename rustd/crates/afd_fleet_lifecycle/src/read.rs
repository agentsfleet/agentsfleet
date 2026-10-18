//! What a workspace's fleets look like when read: the page walk and the ingress status.
//!
//! # The page decides `more` by over-fetching, not by being full
//!
//! A token emitted whenever the page came back FULL hands the last page of an
//! exactly-divisible walk a cursor that resolves to nothing, and costs its
//! client one extra request to discover that. Here the walk asks for one row
//! past the limit and keeps the limit, so [`FleetPage::more`] is a fact rather
//! than a guess — the same rule the workspace directory follows, and it costs
//! one row per page. A client following this cursor stops on the last page.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::error::{self, Result};
use crate::{FleetStatus, Fleets, sql};

/// The context a failed page walk reports under.
const CONTEXT_PAGE: &str = "list workspace fleets";

/// What the ingress check was doing, for the operator's log line.
const CONTEXT_INGRESS: &str = "read a fleet's ingress status";

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
/// Validity is therefore the wire layer's question: text that will not parse
/// renders as `null`.
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
    pub created_at: Timestamp,
    /// When it last changed. Doubles as the config revision a PATCH echoes.
    pub updated_at: Timestamp,
    /// What may wake it, projected from the head revision's configuration.
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
    pub created_at: Timestamp,
    /// The boundary row's identifier, breaking ties inside one millisecond.
    pub id: Uuid7,
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
        let mut rows = sqlx::query_as::<_, PageRow>(sql::SELECT_FLEET_PAGE)
            .bind(workspace.as_str())
            .bind(fetch)
            .bind(after.map(|boundary| boundary.created_at.to_sqlx()))
            .bind(after.map(|boundary| boundary.id.as_str()))
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_PAGE))?
            .into_iter()
            .map(FleetRow::try_from)
            .collect::<Result<Vec<_>>>()?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        Ok(FleetPage { rows, more })
    }

    /// Whether this fleet will take new work, without reading what it is.
    ///
    /// `Result<Option<_>>`: a fleet this workspace does not hold is an ANSWER
    /// the caller renders as 404, and a datastore that would not say is a
    /// failure. Collapsing them would make an outage look like a deleted fleet.
    ///
    /// Narrow on purpose: deciding whether a message may be posted needs the
    /// status and nothing the setup holds.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a status this build
    /// cannot read.
    pub async fn ingress_status(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> Result<Option<FleetStatus>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_scalar::<_, String>(sql::SELECT_FLEET_STATUS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_INGRESS))?
            .map(|status| parse_status(&status))
            .transpose()
    }
}

/// One page row, by the names [`sql::SELECT_FLEET_PAGE`] gives its columns.
#[derive(Debug, sqlx::FromRow)]
struct PageRow {
    id: String,
    name: String,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    triggers: Option<String>,
    event_count: i64,
    budget_used_nanos: i64,
}

impl TryFrom<PageRow> for FleetRow {
    type Error = crate::Error;

    /// Refuses a status this build does not know: a row written by a newer
    /// daemon is an unreadable row and says so, rather than defaulting to
    /// something this build would then act on.
    fn try_from(row: PageRow) -> Result<Self> {
        Ok(Self {
            status: parse_status(&row.status)?,
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            updated_at: row.updated_at,
            triggers: row.triggers.map(|text| Triggers(text.into_boxed_str())),
            events_processed: row.event_count,
            budget_used_nanos: row.budget_used_nanos,
        })
    }
}

/// A stored status, refusing a spelling this build does not know.
fn parse_status(raw: &str) -> Result<FleetStatus> {
    FleetStatus::parse(raw).ok_or_else(|| error::row_malformed(COLUMN_STATUS, raw))
}
