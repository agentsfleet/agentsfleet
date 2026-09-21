//! The owned collection's statements — one workspace's own entries, and the
//! removal of one of them.
//!
//! # Why these are not the gallery's statements with a predicate added
//!
//! [`super::super::gallery`] merges two libraries into one resumable order, and
//! every complication in it — the tier rank, the union-compatible aliasing, the
//! three-column seek — exists to serve that merge. This collection reads one
//! table, so its order is two columns and its seek is two comparisons. Sharing
//! the gallery's statements would mean carrying a tier rank that is constant
//! here, and filtering a union whose platform arm can never contribute a row.
//!
//! # Both reads project the same columns, and none of them is bundle content
//!
//! `skill_markdown`, `trigger_markdown` and `support_files_json` are absent by
//! construction: a read cannot leak what it does not project, and a list of
//! what a workspace owns has no use for the bytes. `content_hash` IS projected
//! — it is the domain key half an operator needs to tell two onboardings of
//! near-identical bundles apart, which is the whole reason this page exists.

/// The projection every owned-entry read answers with.
///
/// Positional, matching the decode beside it: the statement's column order and
/// that function are one definition, and reading by name would let a drifted
/// projection pass unnoticed.
macro_rules! projection {
    () => {
        concat!(
            "SELECT id::text, name, description, source_kind, source_ref,",
            "\n       content_hash, created_at",
            "\n  FROM core.tenant_fleet_library"
        )
    };
}

/// The isolation boundary, and it is not optional.
///
/// Every statement in this file carries it. A read without it lists every
/// workspace's entries; a delete without it removes another workspace's row on
/// a guessed identifier. `entries_are_workspace_scoped` asserts the substring
/// on each statement constant, so one added later without the predicate fails
/// in the fast lane rather than in a cross-tenant incident.
macro_rules! owned_by {
    () => {
        "\n WHERE workspace_id = $1::uuid"
    };
}

/// Newest first, with the identifier breaking ties.
///
/// `COLLATE "C"` is byte order, and it is here for the reason the gallery
/// records: the seek below compares identifiers bytewise, so a locale-sensitive
/// collation would order the page differently from the way the cursor resumes
/// it, and the rows falling between the two orders would never be served.
macro_rules! order_by {
    () => {
        "\n ORDER BY created_at DESC, id COLLATE \"C\" DESC"
    };
}

/// The seek, mirroring that order exactly.
///
/// Both columns descend, so "after" is smaller in both. A predicate disagreeing
/// with its `ORDER BY` does not error — it silently skips or repeats rows at
/// every page boundary.
macro_rules! seek {
    () => {
        "\n   AND (created_at < $2 OR (created_at = $2 AND id COLLATE \"C\" < $3))"
    };
}

/// The first page of a workspace's own entries.
///
/// `$1` workspace · `$2` how many rows to fetch, which is `limit + 1`: the
/// extra row never reaches the response, it only answers "is there another
/// page?" without a second `COUNT`.
pub(super) const FIRST_PAGE: &str = concat!(projection!(), owned_by!(), order_by!(), "\n LIMIT $2");

/// Later pages.
///
/// `$1` workspace · `$2` boundary instant · `$3` boundary id · `$4` how many.
pub(super) const PAGE_AFTER: &str = concat!(
    projection!(),
    owned_by!(),
    seek!(),
    order_by!(),
    "\n LIMIT $4"
);

/// Removing one entry this workspace owns.
///
/// Scoped on both halves on purpose. `id` alone would delete another
/// workspace's row for a caller who guessed a UUID, and the `workspace_id`
/// predicate is what makes a foreign identifier indistinguishable from an
/// already-removed one — the handler answers `204` either way rather than
/// running a second unscoped read whose only effect would be to confirm that
/// the identifier exists somewhere.
///
/// `RETURNING` nothing: the row count is the whole answer, and the caller acts
/// the same way on zero and one.
pub(super) const REMOVE_ENTRY: &str = "\
DELETE FROM core.tenant_fleet_library
 WHERE id = $1::uuid AND workspace_id = $2::uuid";

#[cfg(test)]
mod tests {
    use super::{FIRST_PAGE, PAGE_AFTER, REMOVE_ENTRY};

    /// Every statement against the tenant library carries a workspace predicate.
    ///
    /// Invariant 1 of the spec, made mechanical. This is the test a statement
    /// added later without the predicate fails.
    #[test]
    fn test_tenant_library_delete_statement_is_workspace_scoped() {
        for statement in [FIRST_PAGE, PAGE_AFTER, REMOVE_ENTRY] {
            assert!(
                statement.contains("workspace_id = $1::uuid")
                    || statement.contains("workspace_id = $2::uuid"),
                "a tenant-library statement without a workspace predicate reads \
                 or removes another workspace's rows"
            );
        }
        assert!(REMOVE_ENTRY.contains("id = $1::uuid AND workspace_id = $2::uuid"));
    }

    /// The removal touches one table and removes at most one row.
    #[test]
    fn the_removal_is_one_row_of_one_table() {
        assert_eq!(REMOVE_ENTRY.matches("DELETE FROM").count(), 1);
        assert!(REMOVE_ENTRY.contains("core.tenant_fleet_library"));
        assert!(!REMOVE_ENTRY.contains("RETURNING"));
    }

    /// Both pages walk the same order, and the seek follows it.
    #[test]
    fn both_pages_walk_the_same_order_and_the_seek_mirrors_it() {
        let order = "ORDER BY created_at DESC, id COLLATE \"C\" DESC";
        assert!(FIRST_PAGE.contains(order));
        assert!(PAGE_AFTER.contains(order));
        assert!(PAGE_AFTER.contains("created_at < $2"));
        assert!(PAGE_AFTER.contains("created_at = $2 AND id COLLATE \"C\" < $3"));
        // Byte order in one and a locale in the other serves neither the page
        // nor the resumption correctly. Both comparisons collate the same way.
        assert_eq!(PAGE_AFTER.matches("COLLATE \"C\"").count(), 2);
    }

    /// Neither read projects bundle content.
    ///
    /// A read cannot leak through a column it does not select. `content_hash`
    /// is deliberately present — it is what tells two near-identical
    /// onboardings apart — and the three document columns are deliberately not.
    #[test]
    fn the_owned_reads_carry_no_bundle_content() {
        for statement in [FIRST_PAGE, PAGE_AFTER] {
            for column in ["skill_markdown", "trigger_markdown", "support_files_json"] {
                assert!(
                    !statement.contains(column),
                    "the owned collection projects {column}"
                );
            }
            assert!(statement.contains("content_hash"));
        }
    }

    /// The platform library is not reachable from this collection.
    #[test]
    fn the_owned_collection_reads_only_the_tenant_table() {
        for statement in [FIRST_PAGE, PAGE_AFTER, REMOVE_ENTRY] {
            assert!(statement.contains("core.tenant_fleet_library"));
            assert!(!statement.contains("FROM core.fleet_library"));
            assert!(!statement.contains("UNION"));
        }
    }
}
