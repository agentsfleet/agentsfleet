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

/// Continuation indent for the search disjunction's arms.
const OR_INDENT = "\n    OR ";

/// Paired with every LIKE the gallery builds, whose escape character this must
/// match or the escaping in `query.likeContains` is inert.
const GALLERY_LIKE_ESCAPE =
    \\ ESCAPE '\'
;

/// One searchable column, folded by the SAME expression as the needle.
///
/// NFKC and casefold are SQL-side: Zig's standard library ships no Unicode
/// normalization tables and Postgres `normalize(text, NFKC)` is built in and
/// IMMUTABLE, so `lower(normalize(col, NFKC))` is index-eligible. Generated from
/// one function rather than written out three times — a column folded even
/// slightly differently from the needle matches by accident.
fn foldedLike(comptime column: []const u8) []const u8 {
    return "lower(normalize(" ++ column ++ ", NFKC)) LIKE lower(normalize($3, NFKC))" ++ GALLERY_LIKE_ESCAPE;
}

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

/// `$3` = the LIKE pattern, or NULL for no search. Matches id, name and
/// description only — §3 names those three.
const GALLERY_WHERE_SEARCH =
    "\n WHERE ($3::text IS NULL" ++
    OR_INDENT ++ foldedLike("id") ++
    OR_INDENT ++ foldedLike("name") ++
    OR_INDENT ++ foldedLike("description") ++ ")";

/// The merged order. Each direction is load-bearing and they differ:
/// `created_at` newest-first, `tier_rank` platform-first, `id` descending.
const ORDER_BY_GALLERY =
    "\n ORDER BY created_at DESC, tier_rank ASC, id" ++ COLLATE_C ++ " DESC";

/// The seek predicate, mirroring `fleet_keyset.follows` exactly. Note each
/// comparison follows its OWN sort direction — `created_at` and `id` descend so
/// "after" is SMALLER, `tier_rank` ascends so "after" is LARGER. A predicate that
/// disagrees with its ORDER BY does not error; it silently skips or repeats rows
/// at every page boundary.
const GALLERY_SEEK =
    "\n   AND (created_at < $4" ++
    "\n     OR (created_at = $4 AND tier_rank > $5)" ++
    "\n     OR (created_at = $4 AND tier_rank = $5 AND id" ++ COLLATE_C ++ " < $6))";

/// First page. `$4` is `limit + 1`: the extra row never reaches the response, it
/// only answers "is there another page?" without a second COUNT.
pub const SELECT_GALLERY_PAGE_FIRST =
    GALLERY_MERGED ++ GALLERY_WHERE_SEARCH ++ ORDER_BY_GALLERY ++ "\n LIMIT $4";

/// Resume after a cursor: `$4` created_at, `$5` tier_rank, `$6` id, `$7` limit+1.
pub const SELECT_GALLERY_PAGE_AFTER =
    GALLERY_MERGED ++ GALLERY_WHERE_SEARCH ++ GALLERY_SEEK ++ ORDER_BY_GALLERY ++ "\n LIMIT $7";

/// Platform detail — one entry, every field the summary sheds. Carries the same
/// publish+bundle pair as the collection arm, so an entry that is invisible in
/// the gallery is also unreachable here rather than merely unlisted.
pub const SELECT_GALLERY_DETAIL_PLATFORM =
    \\SELECT id, name, description, source_repo, created_at,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text,
    \\       COALESCE(support_files_json::text, '[]'), (trigger_markdown IS NOT NULL)
    \\  FROM core.fleet_library
    \\ WHERE id = $1 AND visibility = $2 AND content_hash IS NOT NULL
;

/// Tenant detail, scoped to the workspace. A foreign id returns no row, so the
/// handler answers the same 404 it gives a genuinely absent one — the response
/// cannot be used to enumerate another workspace's entries.
pub const SELECT_GALLERY_DETAIL_TENANT =
    \\SELECT id::text, name, description, source_ref, created_at,
    \\       requirements_json::text, support_files_json::text
    \\  FROM core.tenant_fleet_library
    \\ WHERE id = $1::uuid AND workspace_id = $2::uuid
;
