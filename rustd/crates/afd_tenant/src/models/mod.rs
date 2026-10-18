//! The priced model catalogue, as the tenant plane reads it.
//!
//! `model.platform_models` prices the platform's billing spine; every
//! authenticated tenant may read it and nobody on this plane may write it —
//! the admin CRUD is M179's surface. What lives here is one bounded page walk
//! in the catalogue's normalized order.
//!
//! # No response cache
//!
//! Every read is served from the page statement directly, with no
//! revision-keyed in-process page cache in front of it. Such a cache is a
//! performance subsystem the wire — envelope, refusals, validators — cannot
//! see, so it waits for a benchmark that asks for it. Because nothing reads
//! the revision, a catalogue whose REVISION row alone is unreadable still
//! serves.

pub mod cursor;

use afd_db::Db;
use jiff::Timestamp;
use sqlx::FromRow as _;

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

        // Decoded after the fetch rather than by `query_as`, so a row this
        // daemon cannot read reports as that, not as the catalogue being
        // unavailable.
        let served = fetched
            .iter()
            .take(limit as usize)
            .map(|row| CatalogueRow::from_row(row).map_err(unreadable_row))
            .collect::<Result<Vec<_>>>()?;
        let max_updated = served.iter().map(|row| row.updated_at).max();
        let boundary = served.last().map(|row| Boundary {
            display_key: row.display_key.clone(),
            vendor_key: row.vendor_key.clone(),
            id: row.id.clone(),
        });
        let models = served.into_iter().map(CatalogueRow::served).collect();
        Ok(LibraryPage {
            models,
            max_updated,
            boundary,
            has_more: fetched.len() > limit as usize,
        })
    }
}

/// One catalogue row as the library read serves it, plus the version input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRow {
    /// The model's identifier — the wire calls it `id`, and it is the
    /// `model_name` column: the row's own id never leaves the admin plane.
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
    pub updated_at: Timestamp,
}

/// One page, and what the next cursor is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPage {
    /// The rows on this page, in the catalogue's normalized order.
    pub models: Vec<LibraryRow>,
    /// The greatest `updated_at` among the served rows; `None` for an empty
    /// page.
    pub max_updated: Option<Timestamp>,
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
    /// `lower(normalize(model_name, NFKC))`, as the statement computed it.
    pub display_key: String,
    /// `lower(normalize(provider, NFKC))`, likewise.
    pub vendor_key: String,
    /// The row id breaking ties between folded twins.
    pub id: String,
}

/// The context a column this daemon cannot read reports under.
const CONTEXT_ROW: &str = "read catalogue row";

/// One row that will not decode, with this walk's context.
fn unreadable_row(source: sqlx::Error) -> crate::Error {
    error::query(CONTEXT_ROW)(source)
}

/// One catalogue row, by column name: the six wire columns, the version input,
/// and the walk position the next cursor is built from.
#[derive(Debug, sqlx::FromRow)]
struct CatalogueRow {
    model_name: String,
    provider: String,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    id: String,
    display_key: String,
    vendor_key: String,
}

impl CatalogueRow {
    /// The row as the library read serves it.
    fn served(self) -> LibraryRow {
        LibraryRow {
            model_id: self.model_name,
            provider: self.provider,
            context_cap_tokens: self.context_cap_tokens,
            input_nanos_per_mtok: self.input_nanos_per_mtok,
            cached_input_nanos_per_mtok: self.cached_input_nanos_per_mtok,
            output_nanos_per_mtok: self.output_nanos_per_mtok,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    /// A catalogue row that will not read keeps this walk's context.
    ///
    /// `unreadable_row` exists so every row in the walk reports under one
    /// context rather than each site inventing its own — so what is worth
    /// holding is that it lifts a real `sqlx::Error` without dropping the cause
    /// the operator needs.
    #[test]
    fn an_unreadable_catalogue_row_keeps_its_context_and_cause() {
        use std::error::Error as _;

        let failure = super::unreadable_row(sqlx::Error::PoolClosed);

        assert!(failure.source().is_some(), "the sqlx cause survives");
        assert_ne!(failure.to_string(), "");
        assert_ne!(failure.code().as_str(), "");
    }
}
