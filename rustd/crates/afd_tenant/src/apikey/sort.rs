//! The orderings the api-key list offers, as a closed vocabulary.
//!
//! # Why the vocabulary lives beside the statement
//!
//! An ORDER BY clause cannot be a bind parameter, so it is interpolated — and
//! the only thing that makes that safe is that the clause comes from a value
//! nothing but [`ApiKeySort::parse`] can produce. Keeping the enum in the crate
//! that owns the statement means the clause and the columns it names are read
//! together; keeping it out of the HTTP layer means no handler is in a position
//! to assemble one.

use afd_core::paging::{BoundaryKind, Comparator, SortOrder};

/// How a tenant's api-keys are ordered.
///
/// Four orderings, matching `sortSpecFor`'s allowlist exactly — the dashboard
/// sends these spellings, so they are wire values rather than a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeySort {
    /// Oldest first.
    CreatedAscending,
    /// Newest first — what a caller naming nothing gets.
    CreatedDescending,
    /// By name, A to Z.
    NameAscending,
    /// By name, Z to A.
    NameDescending,
}

impl SortOrder for ApiKeySort {
    /// Newest first. A key list is read to find the one just minted far more
    /// often than to audit the oldest.
    const DEFAULT: Self = Self::CreatedDescending;

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "created_at" => Some(Self::CreatedAscending),
            "-created_at" => Some(Self::CreatedDescending),
            "key_name" => Some(Self::NameAscending),
            "-key_name" => Some(Self::NameDescending),
            _ => None,
        }
    }

    /// Every clause ends in the row id, and that is not decoration: two keys
    /// minted in the same millisecond, or sharing a name across a rename, have
    /// no order between them otherwise — and a seek past that boundary drops
    /// one of them from every page it could appear on.
    fn order_by(self) -> &'static str {
        match self {
            Self::CreatedAscending => "created_at ASC, id ASC",
            Self::CreatedDescending => "created_at DESC, id DESC",
            Self::NameAscending => "key_name ASC, id ASC",
            Self::NameDescending => "key_name DESC, id DESC",
        }
    }

    fn comparator(self) -> Comparator {
        match self {
            Self::CreatedAscending | Self::NameAscending => Comparator::Ascending,
            Self::CreatedDescending | Self::NameDescending => Comparator::Descending,
        }
    }

    fn boundary(self) -> BoundaryKind {
        match self {
            Self::CreatedAscending | Self::CreatedDescending => BoundaryKind::Timestamp,
            Self::NameAscending | Self::NameDescending => BoundaryKind::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_is_the_four_spellings_the_dashboard_sends() {
        let accepted = [
            ("created_at", ApiKeySort::CreatedAscending),
            ("-created_at", ApiKeySort::CreatedDescending),
            ("key_name", ApiKeySort::NameAscending),
            ("-key_name", ApiKeySort::NameDescending),
        ];
        for (raw, expected) in accepted {
            assert_eq!(ApiKeySort::parse(raw), Some(expected), "sort {raw:?}");
        }
        for raw in ["id", "-id", "key_hash", "created_at ASC", "", "CREATED_AT"] {
            assert_eq!(ApiKeySort::parse(raw), None, "sort {raw:?}");
        }
    }

    #[test]
    fn no_ordering_names_the_digest_column() {
        // A sort by `key_hash` would let a caller binary-search the stored
        // digest one page at a time, which is an offline attack served over
        // HTTP. The allowlist is the only thing stopping it, so this asserts
        // the allowlist rather than trusting it.
        for sort in [
            ApiKeySort::CreatedAscending,
            ApiKeySort::CreatedDescending,
            ApiKeySort::NameAscending,
            ApiKeySort::NameDescending,
        ] {
            assert!(!sort.order_by().contains("key_hash"), "{sort:?}");
            assert!(sort.order_by().ends_with("id ASC") || sort.order_by().ends_with("id DESC"));
        }
    }

    #[test]
    fn a_boundary_kind_follows_the_column_the_clause_names() {
        for sort in [ApiKeySort::CreatedAscending, ApiKeySort::CreatedDescending] {
            assert_eq!(sort.boundary(), BoundaryKind::Timestamp);
            assert!(sort.order_by().starts_with("created_at"));
        }
        for sort in [ApiKeySort::NameAscending, ApiKeySort::NameDescending] {
            assert_eq!(sort.boundary(), BoundaryKind::Text);
            assert!(sort.order_by().starts_with("key_name"));
        }
    }

    #[test]
    fn the_comparator_follows_the_direction() {
        for sort in [ApiKeySort::CreatedAscending, ApiKeySort::NameAscending] {
            assert_eq!(sort.comparator().as_sql(), ">");
            assert!(sort.order_by().contains("ASC"));
        }
        for sort in [ApiKeySort::CreatedDescending, ApiKeySort::NameDescending] {
            assert_eq!(sort.comparator().as_sql(), "<");
            assert!(sort.order_by().contains("DESC"));
        }
    }
}
