//! Metadata-only API key pages and their cursor boundaries.
use super::{ApiKeySort, row_unreadable};
use crate::Result;
use afd_core::paging::{Boundary, BoundaryKind, Cursor, SortOrder as _};
use jiff::Timestamp;
use sqlx::FromRow as _;
use sqlx::postgres::PgRow;

impl Boundary<ApiKeySort> for KeyRow {
    /// Switches on the SORT's declared boundary kind, not on its variants.
    ///
    /// [`ApiKeySort::order_by`] and [`ApiKeySort::boundary`] are methods on one
    /// enum, so a new ordering cannot name a column here and a different one
    /// there — the compiler makes the pair move together.
    fn cursor(&self, sort: ApiKeySort) -> Cursor {
        match sort.boundary() {
            BoundaryKind::Timestamp => Cursor::Timestamp {
                at: self.created_at,
                id: self.id.clone(),
            },
            BoundaryKind::Text => Cursor::Text {
                value: self.name.clone(),
                id: self.id.clone(),
            },
        }
    }
}

/// One page of a tenant's keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    /// The keys on this page, in the requested order.
    pub keys: Vec<KeyRow>,
    /// How many keys the tenant holds in total, across every page.
    ///
    /// Page-stable: the count subquery carries no keyset predicate, so a client
    /// walking pages sees one number rather than a shrinking one.
    pub total: i64,
}

impl Listing {
    /// Reads the page out of the rows the lateral join produced.
    ///
    /// The join guarantees at least one row even for an empty page — a marker
    /// carrying the real total and null key columns — so the total is read from
    /// the first row and a null identifier means "no keys" rather than a
    /// malformed one. Each row is decoded in two steps for that reason: the
    /// marker's columns first, then the key's, only where an identifier says
    /// a key is there.
    pub(super) fn of(rows: &[PgRow]) -> Result<Self> {
        // No rows at all cannot happen while the lateral join stands; were it
        // to, a total of zero and no keys is what an empty list means anyway,
        // so it folds into the same read rather than taking a branch of its own.
        let total = rows
            .first()
            .map(MarkerRow::from_row)
            .transpose()
            .map_err(row_unreadable)?
            .map_or(0, |marker| marker.total);
        let mut keys = Vec::with_capacity(rows.len());
        for row in rows {
            if MarkerRow::from_row(row)
                .map_err(row_unreadable)?
                .id
                .is_some()
            {
                keys.push(ListedRow::from_row(row).map_err(row_unreadable)?.into());
            }
        }
        Ok(Self { keys, total })
    }
}

/// One key, as a list shows it.
///
/// Metadata only. There is no field here that could carry the digest, which is
/// the structural half of "revealed exactly once" — the wire shape cannot hold
/// a secret even if a statement were changed to select one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    /// The key's identifier.
    pub id: String,
    /// What it is called.
    pub name: String,
    /// When it was minted.
    pub created_at: jiff::Timestamp,
    /// When it stopped working, if it has.
    pub revoked_at: Option<jiff::Timestamp>,
}

impl KeyRow {
    /// Whether it still authenticates: a key is live until it is revoked.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// What every row of a page carries, the empty-page marker included: the
/// tenant's total, and the key identifier that is NULL only on the marker.
#[derive(Debug, sqlx::FromRow)]
struct MarkerRow {
    id: Option<String>,
    total: i64,
}

/// One key's columns, read only from a row that holds a key.
#[derive(Debug, sqlx::FromRow)]
struct ListedRow {
    id: String,
    name: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    revoked_at: Option<jiff_sqlx::Timestamp>,
}

impl From<ListedRow> for KeyRow {
    fn from(row: ListedRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            revoked_at: row.revoked_at.map(jiff_sqlx::Timestamp::to_jiff),
        }
    }
}
