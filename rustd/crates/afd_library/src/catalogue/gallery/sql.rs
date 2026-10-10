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
//! # The tier ranks are literals here, and a live walk is what keeps them honest
//!
//! An `ORDER BY` cannot take a bind parameter and `concat!` takes literals
//! only, so the ranks [`Tier`](super::Tier) owns are spelled again in the SQL.
//! Assembling the statement at runtime instead would make it a `String`, which
//! `sqlx` refuses without an injection audit — correctly, since a statement
//! built per request is the shape that surface lives in. So the statements stay
//! compile-time constants, and drift is caught by running them: a rank that no
//! longer matches [`Tier::from_rank`](super::Tier::from_rank) mislabels a card,
//! and a seek that disagrees with the order skips a row at a tied instant —
//! both of which the catalogue lane's tie walk reports.

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
