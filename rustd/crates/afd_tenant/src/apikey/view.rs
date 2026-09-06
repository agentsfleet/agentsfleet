//! Metadata-only API key pages and their cursor boundaries.
use super::{ApiKeySort, COLUMN_REVOKED_AT, row_unreadable};
use crate::Result;
use afd_core::paging::{Boundary, BoundaryKind, Cursor, SortOrder as _};
use sqlx::Row as _;

impl Boundary<ApiKeySort> for KeyRow {
    /// Switches on the SORT's declared boundary kind, not on its variants.
    ///
    /// [`ApiKeySort::order_by`] and [`ApiKeySort::boundary`] are methods on one
    /// enum, so a new ordering cannot name a column here and a different one
    /// there — the compiler makes the pair move together.
    fn cursor(&self, sort: ApiKeySort) -> Cursor {
        match sort.boundary() {
            BoundaryKind::Timestamp => Cursor::Timestamp {
                at_ms: self.created_at_ms,
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
    /// malformed one.
    pub(super) fn of(rows: &[sqlx::postgres::PgRow]) -> Result<Self> {
        let Some(first) = rows.first() else {
            // Unreachable while the lateral join stands, and answered rather
            // than reported: a tenant with no keys and a statement that
            // answered nothing look identical to a caller, and both mean the
            // list is empty.
            return Ok(Self::default());
        };
        let total: i64 = first.try_get("total").map_err(row_unreadable)?;
        let mut keys = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(key) = KeyRow::of(row)? {
                keys.push(key);
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
    /// Whether it still authenticates.
    pub active: bool,
    /// When it was minted.
    pub created_at_ms: i64,
    /// When it last authenticated, if it ever has.
    pub last_used_at_ms: Option<i64>,
    /// When it stopped working, if it has.
    pub revoked_at_ms: Option<i64>,
}

impl KeyRow {
    /// One row, or `None` for the empty-page marker.
    fn of(row: &sqlx::postgres::PgRow) -> Result<Option<Self>> {
        let id: Option<String> = row.try_get("id").map_err(row_unreadable)?;
        let Some(id) = id else {
            return Ok(None);
        };
        Ok(Some(Self {
            id,
            name: row.try_get("key_name").map_err(row_unreadable)?,
            active: row.try_get("active").map_err(row_unreadable)?,
            created_at_ms: row.try_get("created_at").map_err(row_unreadable)?,
            last_used_at_ms: row.try_get("last_used_at").map_err(row_unreadable)?,
            revoked_at_ms: row.try_get(COLUMN_REVOKED_AT).map_err(row_unreadable)?,
        }))
    }
}
