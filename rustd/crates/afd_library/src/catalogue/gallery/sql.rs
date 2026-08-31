//! The merged gallery's statements — one page across two libraries.
//!
//! # One statement, not two reads stitched together
//!
//! The predecessor read every published platform row and every tenant row for
//! the workspace and concatenated them, so a workspace's page size was whatever
//! the two tables happened to hold and its "order" was two orders stapled
//! together. The merge is not an optimization: a keyset boundary has to be
//! resolvable against the COMBINED sequence, and neither half knows where the
//! other's rows fall, so two independently paged queries cannot produce a
//! resumable total order at all.
//!
//! # Both arms project the same columns under the same names
//!
//! `UNION ALL` requires it, and the outer `ORDER BY` reads the aliases. The
//! platform table names its repository `source_repo` and keys rows by a slug;
//! the tenant table names it `source_ref` and keys rows by a UUID. Both are
//! aliased and cast so the union's column list is identical and the id tiebreak
//! compares like with like.
//!
//! # The tier ranks are literals here, and a test is what keeps them honest
//!
//! An `ORDER BY` cannot take a bind parameter and `concat!` takes literals
//! only, so the ranks [`Tier`](super::Tier) owns are spelled again in the SQL.
//! Assembling the statement at runtime instead would make it a `String`, which
//! `sqlx` refuses without an injection audit — correctly, since a statement
//! built per request is the shape that surface lives in. So the statements stay
//! compile-time constants and the drift is caught where drift between a literal
//! and a value is always caught: a test that compares them.

/// The platform arm: published rows that actually carry a bundle.
///
/// `visibility = $1 AND content_hash IS NOT NULL` is the same pair the install
/// read enforces, so what a tenant can SEE here is exactly what it can install.
/// `$1` is bound rather than inlined because the published spelling is a value
/// this crate owns, not a literal this statement gets to restate.
macro_rules! platform_arm {
    () => {
        concat!(
            "\n    SELECT id, name, description, source_repo AS source_ref, created_at,",
            "\n           required_credentials::text, required_tools::text, network_hosts::text,",
            "\n           required_credentials_reasons::text, (trigger_markdown IS NOT NULL) AS trigger_present, 0 AS tier_rank",
            "\n      FROM core.fleet_library",
            "\n     WHERE visibility = $1 AND content_hash IS NOT NULL"
        )
    };
}

/// The tenant arm, scoped to the caller's workspace — the isolation boundary.
///
/// The tenant table stores requirements as ONE blob and derives no reasons, so
/// its columns are projected out of that blob and an empty object, which is what
/// keeps both arms union-compatible. `id` is a UUID here and TEXT on the
/// platform side, so it is cast.
macro_rules! tenant_arm {
    () => {
        concat!(
            "\n    SELECT id::text AS id, name, description, source_ref, created_at,",
            "\n           requirements_json->>'credentials', requirements_json->>'tools',",
            "\n           requirements_json->>'network_hosts',",
            "\n           '{}', COALESCE((requirements_json->>'trigger_present')::bool, false), 1 AS tier_rank",
            "\n      FROM core.tenant_fleet_library",
            "\n     WHERE workspace_id = $2::uuid"
        )
    };
}

/// The merged projection.
///
/// Carries `requirements` and `required_credentials_reasons` — both are RENDERED
/// on the card (the credential chips, and the install gate's "why this fleet
/// needs it" copy), so dropping them would cost real information at the moment
/// someone decides whether to install.
///
/// Deliberately WITHOUT the support-file manifest. It is bounded at 32 paths of
/// 160 bytes, so a row carries up to ~6.3 KB of it and a hundred-row page ~630
/// KB — past the body ceiling on its own. And nothing consumes it: no component
/// renders it, the install flow ignores it, and the runner materializes real
/// support-file BYTES out of object storage from the lease's bundle hash. It is
/// the one field of the three with no reader, which is why it is the one that
/// goes.
macro_rules! merged {
    () => {
        concat!(
            "SELECT id, name, description, source_ref, created_at,",
            "\n       required_credentials, required_tools, network_hosts,",
            "\n       required_credentials_reasons, trigger_present, tier_rank",
            "\n  FROM (",
            platform_arm!(),
            "\n    UNION ALL",
            tenant_arm!(),
            "\n  ) g"
        )
    };
}

/// The merged order. Each direction is load-bearing and they differ.
///
/// `created_at` newest-first, `tier_rank` platform-first, `id` descending. The
/// tier participates because merging two tables needs a total order across
/// both, and ordering on the tier LABEL would make `platform` before `tenant` an
/// alphabetical coincidence that inverts the day a third tier is named.
///
/// `COLLATE "C"` is byte order, and it is here because the seek below compares
/// ids bytewise: a locale-sensitive collation would order the page differently
/// from the way the cursor resumes it, and the rows that fall between the two
/// orders would simply never be served.
macro_rules! order_by {
    () => {
        "\n ORDER BY created_at DESC, tier_rank ASC, id COLLATE \"C\" DESC"
    };
}

/// The seek predicate, which must mirror the order above exactly.
///
/// Each comparison follows its OWN direction: `created_at` and `id` descend so
/// "after" is SMALLER, `tier_rank` ascends so "after" is LARGER. A predicate
/// that disagrees with its `ORDER BY` does not error — it silently skips or
/// repeats rows at every page boundary, which is the single most common
/// keyset-pagination bug, and is why the disjunction is written once here
/// rather than at a call site.
macro_rules! seek {
    () => {
        concat!(
            "\n WHERE (created_at < $3",
            "\n     OR (created_at = $3 AND tier_rank > $4)",
            "\n     OR (created_at = $3 AND tier_rank = $4 AND id COLLATE \"C\" < $5))"
        )
    };
}

/// The gallery's first page.
///
/// `$1` published spelling · `$2` workspace · `$3` how many rows to fetch.
///
/// `$3` is `limit + 1`: the extra row never reaches the response, it only
/// answers "is there another page?" without a second `COUNT`.
pub(super) const FIRST_PAGE: &str = concat!(merged!(), order_by!(), "\n LIMIT $3");

/// The gallery's later pages.
///
/// `$1` published spelling · `$2` workspace · `$3` boundary instant ·
/// `$4` boundary tier rank · `$5` boundary id · `$6` how many rows.
pub(super) const PAGE_AFTER: &str = concat!(merged!(), seek!(), order_by!(), "\n LIMIT $6");

#[cfg(test)]
mod tests {
    use super::super::Tier;
    use super::{FIRST_PAGE, PAGE_AFTER};

    /// The platform tier's sort rank, as the statements above spell it.
    ///
    /// Declared here rather than beside them because it is the test's
    /// EXPECTATION, not an input: the statements carry the digit inline, since
    /// `concat!` takes literals only and an `ORDER BY` takes no bind parameter.
    const RANK_PLATFORM: &str = "0";

    /// The tenant tier's sort rank, likewise.
    const RANK_TENANT: &str = "1";

    #[test]
    fn the_ranks_in_the_statements_are_the_ranks_the_type_owns() {
        // The one drift a compile-time statement cannot prevent on its own. If
        // `Tier` renumbers and these literals do not, the page orders one way
        // and the cursor resumes another, and rows disappear at every boundary.
        assert_eq!(RANK_PLATFORM, Tier::Platform.rank().to_string());
        assert_eq!(RANK_TENANT, Tier::Tenant.rank().to_string());
        for (rank, statement) in [(RANK_PLATFORM, FIRST_PAGE), (RANK_TENANT, FIRST_PAGE)] {
            assert!(
                statement.contains(&format!("{rank} AS tier_rank")),
                "the statement projects no {rank} rank"
            );
        }
    }

    #[test]
    fn both_pages_read_both_libraries_and_neither_reads_a_third() {
        for statement in [FIRST_PAGE, PAGE_AFTER] {
            assert!(statement.contains("FROM core.fleet_library"));
            assert!(statement.contains("FROM core.tenant_fleet_library"));
            assert!(statement.contains("UNION ALL"));
        }
    }

    #[test]
    fn the_tenant_arm_is_scoped_to_one_workspace() {
        // The isolation boundary. Losing this predicate turns a workspace's
        // gallery into every workspace's, and every row of it lists.
        for statement in [FIRST_PAGE, PAGE_AFTER] {
            assert!(statement.contains("WHERE workspace_id = $2::uuid"));
        }
    }

    #[test]
    fn the_platform_arm_shows_only_what_a_tenant_could_install() {
        // Seeing a row a tenant cannot install is an install button that fails
        // after the click. The pair is the same one the install read enforces.
        for statement in [FIRST_PAGE, PAGE_AFTER] {
            assert!(statement.contains("WHERE visibility = $1 AND content_hash IS NOT NULL"));
        }
    }

    #[test]
    fn both_pages_walk_the_same_order() {
        // Two pages of one walk ordered differently silently drop rows and
        // repeat others, and the client sees neither happen.
        let order = "ORDER BY created_at DESC, tier_rank ASC, id COLLATE \"C\" DESC";
        assert!(FIRST_PAGE.contains(order));
        assert!(PAGE_AFTER.contains(order));
    }

    #[test]
    fn the_seek_follows_each_column_in_its_own_direction() {
        // The bug this pins: `created_at` and `id` descend so "after" is
        // smaller, `tier_rank` ascends so "after" is larger. Getting one arm
        // backwards does not error — it drops rows at every page boundary.
        assert!(PAGE_AFTER.contains("created_at < $3"));
        assert!(PAGE_AFTER.contains("created_at = $3 AND tier_rank > $4"));
        assert!(PAGE_AFTER.contains("tier_rank = $4 AND id COLLATE \"C\" < $5"));
    }

    #[test]
    fn the_id_tiebreak_collates_the_same_way_it_seeks() {
        // Byte order in one and a locale in the other orders the page
        // differently from the way the cursor resumes it, and the rows between
        // the two orders are never served.
        assert_eq!(PAGE_AFTER.matches("COLLATE \"C\"").count(), 2);
    }

    #[test]
    fn the_page_carries_no_bundle_content_and_no_object_store_key() {
        // A read cannot leak bundle content through a column it does not
        // project. The support-file manifest is absent for a different reason —
        // it has no reader and would not fit the body ceiling.
        for statement in [FIRST_PAGE, PAGE_AFTER] {
            for column in ["skill_markdown", "support_files_json", "content_hash,"] {
                assert!(!statement.contains(column), "the gallery projects {column}");
            }
        }
    }
}
