//! The priced model catalogue, as the tenant plane reads it.
//!
//! `core.model_library` prices the platform's billing spine; every
//! authenticated tenant may read it and nobody on this plane may write it —
//! the admin CRUD is M179's surface. What lives here is one bounded page walk
//! in the catalogue's normalized order.
//!
//! # The declared deviation: no response cache
//!
//! `model_library.zig` serves this read through a revision-keyed in-process
//! page cache, so a hit costs one statement (the revision) and a miss two.
//! This port serves every read from the page statement directly: the wire —
//! envelope, refusals, validators — is identical, and the cache is a
//! performance subsystem the cutover oracle cannot see. It lands as a
//! follow-up against a benchmark rather than inside the port; the milestone's
//! Discovery log carries the decision. One visible edge moves with it: a
//! catalogue whose REVISION row alone is unreadable answered `UZ-LIBRARY-004`
//! there and simply serves here, because nothing reads the revision.

pub mod cursor;

use afd_db::Db;
use sqlx::Row as _;

use crate::sql::models as sql;
use crate::{Result, error};

/// One page of the walk may carry at most this many rows, and asks for one
/// more — the proof-of-more row the handler never serves.
const OVER_FETCH: i64 = 1;

/// The catalogue read surface.
#[derive(Debug, Clone)]
pub struct Models {
    database: Db,
}

impl Models {
    /// A read surface over `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// One bounded page of the catalogue in its normalized order.
    ///
    /// `filter` is the NORMALIZED provider, when the caller sent one;
    /// `after` is the decoded cursor's boundary, when the caller is resuming.
    ///
    /// # Errors
    /// Reports a datastore that would not answer — under the library family's
    /// own code, because this read has its own sentence — and a row this
    /// daemon cannot read.
    pub async fn page(
        &self,
        filter: Option<&str>,
        after: Option<&Boundary>,
        limit: u32,
    ) -> Result<LibraryPage> {
        let fetch = i64::from(limit).saturating_add(OVER_FETCH);
        let mut connection = self.database.acquire().await?;
        let query = match after {
            None => sqlx::query(sql::SELECT_LIBRARY_PAGE_FIRST)
                .bind(filter)
                .bind(fetch),
            Some(boundary) => sqlx::query(sql::SELECT_LIBRARY_PAGE_AFTER)
                .bind(filter)
                .bind(boundary.display_key.as_str())
                .bind(boundary.vendor_key.as_str())
                .bind(boundary.id.as_str())
                .bind(fetch),
        };
        let fetched = query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::library_page_unavailable)?;

        let mut models = Vec::with_capacity(fetched.len().min(limit as usize));
        let mut max_updated_ms = 0i64;
        let mut boundary = None;
        for row in fetched.iter().take(limit as usize) {
            let projected = read_row(row)?;
            if projected.updated_at_ms > max_updated_ms {
                max_updated_ms = projected.updated_at_ms;
            }
            boundary = Some(Boundary {
                display_key: row.try_get("display_key").map_err(unreadable_row)?,
                vendor_key: row.try_get("vendor_key").map_err(unreadable_row)?,
                id: row.try_get("id").map_err(unreadable_row)?,
            });
            models.push(projected);
        }
        Ok(LibraryPage {
            models,
            max_updated_ms,
            boundary,
            has_more: fetched.len() > limit as usize,
        })
    }
}

/// One catalogue row as the library read serves it, plus the version input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRow {
    /// The model's identifier — the wire calls it `id`, and it is the
    /// `model_id` column: the row's own id never leaves the admin plane.
    pub model_id: String,
    /// The provider hosting it.
    pub provider: String,
    /// The context window the price is quoted for.
    pub context_cap_tokens: i32,
    /// The input rate, in nanos per million tokens.
    pub input_nanos_per_mtok: i64,
    /// The cached-input rate, likewise.
    pub cached_input_nanos_per_mtok: i64,
    /// The output rate, likewise.
    pub output_nanos_per_mtok: i64,
    /// When the row last changed — the max across a page stamps the version.
    pub updated_at_ms: i64,
}

/// One page, and what the next cursor is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPage {
    /// The rows on this page, in the catalogue's normalized order.
    pub models: Vec<LibraryRow>,
    /// The greatest `updated_at` among the served rows; 0 for an empty page.
    pub max_updated_ms: i64,
    /// The last served row's position, or `None` for an empty page.
    pub boundary: Option<Boundary>,
    /// Whether a further page exists, proven by the over-fetched row.
    pub has_more: bool,
}

/// A row's position in the walk: the two folded sort keys and the id tiebreak.
///
/// The id is the ROW's id, which never reaches a response body — it rides the
/// cursor opaquely because the folded key pair is not unique once normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// `lower(normalize(model_id, NFKC))`, as the statement computed it.
    pub display_key: String,
    /// `lower(normalize(provider, NFKC))`, likewise.
    pub vendor_key: String,
    /// The row id breaking ties between folded twins.
    pub id: String,
}

/// The context a column this daemon cannot read reports under.
const CONTEXT_ROW: &str = "read catalogue row";

/// One `try_get` failure, with this walk's context.
fn unreadable_row(source: sqlx::Error) -> crate::Error {
    error::query(CONTEXT_ROW)(source)
}

/// Reads the six wire columns and the version input by name.
fn read_row(row: &sqlx::postgres::PgRow) -> Result<LibraryRow> {
    Ok(LibraryRow {
        model_id: row.try_get("model_id").map_err(unreadable_row)?,
        provider: row.try_get("provider").map_err(unreadable_row)?,
        context_cap_tokens: row.try_get("context_cap_tokens").map_err(unreadable_row)?,
        input_nanos_per_mtok: row
            .try_get("input_nanos_per_mtok")
            .map_err(unreadable_row)?,
        cached_input_nanos_per_mtok: row
            .try_get("cached_input_nanos_per_mtok")
            .map_err(unreadable_row)?,
        output_nanos_per_mtok: row
            .try_get("output_nanos_per_mtok")
            .map_err(unreadable_row)?,
        updated_at_ms: row.try_get("updated_at").map_err(unreadable_row)?,
    })
}
