//! The merged Fleet gallery's SQL — the bounded page across both libraries and
//! the single-entry detail read.
//!
//! Split from `sql.zig` because that file reached the 350-line cap (RULE FLL).
//! The seam is by question: `sql.zig` owns writing to the libraries and
//! resolving one entry for INSTALL; this file owns READING them as a gallery.
//! Both remain grep-able for their table names, which is what the SQL Statement
//! Modules rule is protecting.
//
// ONE statement across both libraries, not two reads stitched together in Zig.
// §3 budgets the Fleet summary at a single database statement, and that is not
// an arbitrary target: a merged total order cannot be produced by paging two
// queries independently. A keyset boundary has to be resolvable against the
// COMBINED sequence, and neither half knows where the other's rows fall.
//
// The predecessor (`SELECT_GALLERY_PLATFORM` + `SELECT_GALLERY_TENANT`) read
// both tables unbounded and concatenated them, so a workspace's page size was
// whatever the two tables happened to hold.

/// Tier ranks, as the merged order sorts them — numbers, not the tier LABEL.
/// Ordering on the label makes `platform` < `tenant` an alphabetical coincidence
/// that inverts the day a third tier is added. Mirrors the `Tier` enum in
/// `http/handlers/library/fleet_keyset.zig`, which owns the same ranks for the
/// seek predicate; the two must agree or the page skips or repeats rows.
const RANK_PLATFORM = "0";
const RANK_TENANT = "1";

/// Byte-order collation on the id tiebreak. The seek predicate compares ids
/// bytewise, so the ORDER BY must too — a locale-sensitive collation would order
/// the page differently from the way the cursor resumes it.
const COLLATE_C = " COLLATE \"C\"";

/// The rank column alias, spelled once so both UNION arms project it under the
/// same name — a mismatch there is a column the outer ORDER BY cannot see.
const AS_TIER_RANK = " AS tier_rank";

// The search disjunction retired with the `q` parameter, taking
// `OR_INDENT`, `GALLERY_LIKE_ESCAPE`, and the `foldedLike` column generator with
// it. What they solved is worth keeping on the record in case a search is ever
// asked for again: NFKC folding and casefolding had to happen SQL-side, on both
// the column and the needle, via the one generated expression — Zig ships no
// Unicode tables, and a column folded even slightly differently from the needle
// matches by accident. The `LIKE` escape ran AFTER the fold, because
// compatibility characters (fullwidth `％`, `＿`, `＼`) fold INTO live wildcards
// and an escape pass before the fold misses them.

/// Platform arm. `source_repo` is projected AS `source_ref` so both arms agree on
/// column names as well as types. The `visibility = $1 AND content_hash IS NOT
/// NULL` pair is the same one `SELECT_PLATFORM_INSTALL` enforces, so what a
/// tenant can SEE here is exactly what it can install (M128 Invariant 2).
const GALLERY_PLATFORM_ARM =
    "\n    SELECT id, name, description, source_repo AS source_ref, created_at," ++
    "\n           required_credentials::text, required_tools::text, network_hosts::text," ++
    "\n           required_credentials_reasons::text, (trigger_markdown IS NOT NULL) AS trigger_present, " ++ RANK_PLATFORM ++ AS_TIER_RANK ++
    "\n      FROM core.fleet_library" ++
    "\n     WHERE visibility = $1 AND content_hash IS NOT NULL";

/// Tenant arm, scoped to the caller's workspace — the isolation boundary. `id` is
/// a UUID here and TEXT on the platform side, so it is cast to keep the UNION's
/// column types identical and the id tiebreak comparable across both.
const GALLERY_TENANT_ARM =
    "\n    SELECT id::text AS id, name, description, source_ref, created_at," ++
    // The tenant table stores requirements as ONE blob and derives no reasons,
    // so its arms are projected out of that blob and an empty object, keeping
    // both UNION arms column-compatible.
    "\n           requirements_json->>'credentials', requirements_json->>'tools'," ++
    "\n           requirements_json->>'network_hosts'," ++
    "\n           '{}', COALESCE((requirements_json->>'trigger_present')::bool, false), " ++ RANK_TENANT ++ AS_TIER_RANK ++
    "\n      FROM core.tenant_fleet_library" ++
    "\n     WHERE workspace_id = $2::uuid";

/// The summary projection.
///
/// Carries `requirements` and `required_credentials_reasons` — both are RENDERED
/// to the user on the card (the credential chips, and the ConnectGate's "why
/// this fleet needs it" copy), so dropping them would cost real information at
/// the moment someone decides whether to install.
///
/// Deliberately WITHOUT `support_files`. §3 asked for all three to move to the
/// detail route on a size argument; measured, only this one earns it. Its
/// manifest is bounded at `MAX_SUPPORT_FILES` (32) x `MAX_SUPPORT_PATH_LEN`
/// (160), so a row carries up to ~6.3 KB of it and a 100-row page ~630 KB —
/// past the 512 KiB ceiling on its own, before anything else is counted. And
/// nothing consumes it: no UI component renders it, the install flow ignores it,
/// and the runner materializes real support-file BYTES from object storage via
/// the lease's bundle hash, never from this manifest. Removing the one field
/// with no reader is what brings the page inside its ceiling; removing the two
/// with readers would have bought little and cost the install gate its copy.
const GALLERY_MERGED =
    "SELECT id, name, description, source_ref, created_at," ++
    "\n       required_credentials, required_tools, network_hosts," ++
    "\n       required_credentials_reasons, trigger_present, tier_rank" ++
    "\n  FROM (" ++ GALLERY_PLATFORM_ARM ++
    "\n    UNION ALL" ++ GALLERY_TENANT_ARM ++
    "\n  ) g";

/// The merged order. Each direction is load-bearing and they differ:
/// `created_at` newest-first, `tier_rank` platform-first, `id` descending.
const ORDER_BY_GALLERY =
    "\n ORDER BY created_at DESC, tier_rank ASC, id" ++ COLLATE_C ++ " DESC";

/// The seek predicate, mirroring `fleet_keyset.follows` exactly. Note each
/// comparison follows its OWN sort direction — `created_at` and `id` descend so
/// "after" is SMALLER, `tier_rank` ascends so "after" is LARGER. A predicate that
/// disagrees with its ORDER BY does not error; it silently skips or repeats rows
/// at every page boundary.
///
/// It opens the WHERE rather than continuing one. This was once an `AND` arm
/// hanging off the search clause; with that clause retired, the seek is the only
/// outer predicate and must introduce its own `WHERE`.
const GALLERY_SEEK =
    "\n WHERE (created_at < $3" ++
    "\n     OR (created_at = $3 AND tier_rank > $4)" ++
    "\n     OR (created_at = $3 AND tier_rank = $4 AND id" ++ COLLATE_C ++ " < $5))";

/// First page. `$3` is `limit + 1`: the extra row never reaches the response, it
/// only answers "is there another page?" without a second COUNT.
///
/// The inner arms bind `$1` (visibility) and `$2` (workspace id); the outer query
/// adds no predicate of its own on the first page.
pub const SELECT_GALLERY_PAGE_FIRST =
    GALLERY_MERGED ++ ORDER_BY_GALLERY ++ "\n LIMIT $3";

/// Resume after a cursor: `$3` created_at, `$4` tier_rank, `$5` id, `$6` limit+1.
pub const SELECT_GALLERY_PAGE_AFTER =
    GALLERY_MERGED ++ GALLERY_SEEK ++ ORDER_BY_GALLERY ++ "\n LIMIT $6";

// The two per-entry detail projections are gone with the route they served. That
// route was built, published, and retired unconsumed — `router.zig` asserts its
// former URL is unrouted and `gallery_keyset_integration_test.zig` pins it to 404
// even for a resident entry. Their only remaining distinction over the summary
// was the support-file manifest, which no reader ever wanted.
