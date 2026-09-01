//! The keyset boundary itself — its two forms, and the string a client holds.
//!
//! Split from `paging.rs` at the file cap, along the line that file's own
//! header draws: the wire form of a cursor is a DATA FORMAT both binaries
//! issue and accept, spelled the way `keyset_cursor.zig` spells it. That is a
//! contract of its own, and it now lives in a file of its own — while
//! `paging.rs` keeps what a REQUEST is parsed into, which is a different job
//! that merely holds one of these.
//!
//! Re-exported from the parent, so `afd_core::paging::Cursor` is still where
//! every caller finds it.

use std::fmt::{self, Display, Formatter};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

use super::{CURSOR_SEPARATOR, TEXT_FORM_PREFIX};

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
    /// Milliseconds since the epoch — a `created_at` ordering.
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
        at_ms: i64,
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
    /// The boundary row's identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Timestamp { id, .. } | Self::Text { id, .. } => id,
        }
    }

    /// Which kind of boundary this carries.
    #[must_use]
    pub const fn kind(&self) -> BoundaryKind {
        match self {
            Self::Timestamp { .. } => BoundaryKind::Timestamp,
            Self::Text { .. } => BoundaryKind::Text,
        }
    }

    /// Reads a cursor this daemon — or the Zig one — issued.
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
        let at_ms = head.parse().map_err(|_digits| InvalidCursor)?;
        Self::of_timestamp(at_ms, rest)
    }

    /// A timestamp-boundary cursor, refusing an empty identifier.
    fn of_timestamp(at_ms: i64, id: &str) -> Result<Self, InvalidCursor> {
        if id.is_empty() {
            return Err(InvalidCursor);
        }
        Ok(Self::Timestamp {
            at_ms,
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
            Self::Timestamp { at_ms, id } => write!(f, "{at_ms}{CURSOR_SEPARATOR}{id}"),
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
