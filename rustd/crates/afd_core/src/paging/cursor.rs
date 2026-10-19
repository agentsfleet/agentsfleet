//! The keyset boundary itself — its two forms, and the string a client holds.
//!
//! Split from `paging.rs` at the file cap, along the line that file's own
//! header draws: the wire form of a cursor is a DATA FORMAT a client holds
//! across deploys. That is a promise of its own, and it lives in a file of its
//! own — while `paging.rs` keeps what a REQUEST is parsed into, which is a
//! different job that merely holds one of these.
//!
//! Re-exported from the parent, so `afd_core::paging::Cursor` is still where
//! every caller finds it.

use std::fmt::{self, Display, Formatter};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use jiff::Timestamp;

use super::{CURSOR_SEPARATOR, INSTANT_END, TEXT_FORM_PREFIX};
use crate::clock::whole_millis;

/// Which direction a keyset seek walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparator {
    /// Ascending: rows after the boundary.
    Ascending,
    /// Descending: rows before it.
    Descending,
}

impl Comparator {
    /// The operator, as it appears in the statement.
    #[must_use]
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::Ascending => ">",
            Self::Descending => "<",
        }
    }
}

/// What a cursor's boundary value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    /// An instant: a `created_at` ordering.
    Timestamp,
    /// A sort key that is text — a `key_name` ordering.
    Text,
}

/// Where a page resumes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// The boundary is an instant.
    Timestamp {
        /// The boundary row's sort value.
        at: Timestamp,
        /// The boundary row's identifier, which breaks the tie.
        id: String,
    },
    /// The boundary is a text sort key.
    Text {
        /// The boundary row's sort value.
        value: String,
        /// The boundary row's identifier, which breaks the tie.
        id: String,
    },
}

impl Cursor {
    /// Which kind of boundary this carries.
    #[must_use]
    pub const fn kind(&self) -> BoundaryKind {
        match self {
            Self::Timestamp { .. } => BoundaryKind::Timestamp,
            Self::Text { .. } => BoundaryKind::Text,
        }
    }

    /// Reads a cursor this daemon issued.
    ///
    /// # Errors
    /// Refuses anything that is not one of the two forms, and an empty
    /// identifier half. Nothing here says WHICH way it was wrong: a cursor is
    /// opaque, and a parser that explained itself would be describing an
    /// internal format to whoever was probing it.
    pub fn parse(raw: &str) -> Result<Self, InvalidCursor> {
        let (head, rest) = raw.split_once(CURSOR_SEPARATOR).ok_or(InvalidCursor)?;
        if head == TEXT_FORM_PREFIX {
            let (encoded, id) = rest.split_once(CURSOR_SEPARATOR).ok_or(InvalidCursor)?;
            let decoded = BASE64.decode(encoded).map_err(|_decode| InvalidCursor)?;
            let value = String::from_utf8(decoded).map_err(|_utf8| InvalidCursor)?;
            return Self::of_text(value, id);
        }
        let end = raw.find(INSTANT_END).ok_or(InvalidCursor)?;
        let (instant, id) = (&raw[..=end], &raw[end + INSTANT_END.len()..]);
        let at: Timestamp = instant.parse().map_err(|_not_an_instant| InvalidCursor)?;
        if whole_millis(at) != at {
            return Err(InvalidCursor);
        }
        Self::of_timestamp(at, id)
    }

    /// A timestamp-boundary cursor, refusing an empty identifier.
    fn of_timestamp(at: Timestamp, id: &str) -> Result<Self, InvalidCursor> {
        if id.is_empty() {
            return Err(InvalidCursor);
        }
        Ok(Self::Timestamp {
            at,
            id: id.to_owned(),
        })
    }

    /// A text-boundary cursor, refusing an empty identifier.
    fn of_text(value: String, id: &str) -> Result<Self, InvalidCursor> {
        if id.is_empty() {
            return Err(InvalidCursor);
        }
        Ok(Self::Text {
            value,
            id: id.to_owned(),
        })
    }
}

impl Display for Cursor {
    /// Writes the form the other binary reads. See the module note.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            // RFC 3339 in UTC, as the API serializes an instant.
            Self::Timestamp { at, id } => write!(f, "{at}{CURSOR_SEPARATOR}{id}"),
            Self::Text { value, id } => {
                let encoded = BASE64.encode(value.as_bytes());
                write!(
                    f,
                    "{TEXT_FORM_PREFIX}{CURSOR_SEPARATOR}{encoded}{CURSOR_SEPARATOR}{id}"
                )
            }
        }
    }
}

/// A `starting_after` value this daemon did not issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCursor;
