//! The values every store speaks: whose memory a call is about, and one entry
//! with everything a copy between stores has to keep.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::memory::{MemoryDelta, Visibility};

/// Whose memory a call reads or writes: the fleet, and the workspace it is in.
///
/// Both halves always travel together, because a store keyed by fleet alone
/// cannot answer for the workspace's shared entries, and the pairing is read
/// once from `core.fleets` rather than trusted from a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner<'a> {
    /// The workspace the fleet belongs to.
    pub workspace: &'a Uuid7,
    /// The fleet — the writer of everything it stores.
    pub fleet: &'a Uuid7,
}

/// One stored entry, with its writer and both instants.
///
/// The writer is part of the identity: two fleets storing one key hold two
/// entries, so a shared fact never conflicts with another fleet's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The fleet that wrote it, and the only fleet that may change it.
    pub fleet: Uuid7,
    /// The key its writer stored it under.
    pub key: String,
    /// What it remembers.
    pub content: String,
    /// The retention category, which decides eviction order.
    pub category: String,
    /// Who reads it.
    pub visibility: Visibility,
    /// When it was first written — the operator page's cursor value.
    pub created_at_ms: i64,
    /// When it was last written — what a copy compares to keep the newer row.
    pub updated_at_ms: i64,
}

impl Record {
    /// This entry as a delta that borrows its text.
    #[must_use]
    pub fn delta(&self) -> MemoryDelta<'_> {
        MemoryDelta {
            key: Cow::Borrowed(&self.key),
            content: Cow::Borrowed(&self.content),
            category: Cow::Borrowed(&self.category),
            visibility: self.visibility,
        }
    }

    /// This entry as a delta, its text moved rather than copied.
    #[must_use]
    pub fn into_delta(self) -> MemoryDelta<'static> {
        MemoryDelta {
            key: Cow::Owned(self.key),
            content: Cow::Owned(self.content),
            category: Cow::Owned(self.category),
            visibility: self.visibility,
        }
    }

    /// Whether `fleet` wrote this entry.
    #[must_use]
    pub fn written_by(&self, fleet: &Uuid7) -> bool {
        &self.fleet == fleet
    }
}

/// What a write's housekeeping removed, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Housekept {
    /// Rows the retention sweep removed.
    pub swept: u64,
    /// Rows evicted to bring the fleet back under its cap.
    pub evicted: u64,
}
