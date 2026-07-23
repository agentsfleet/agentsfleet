//! SQL statement text for the fleet operator-plane handlers (RULE SQLMOD —
//! query text lives here, grepable in one place; siblings with pre-existing
//! inline SQL keep their shape until extracted).

/// The operator runner list: one page, plus a total that survives an offset
/// past the end.
///
/// Pagination happens FIRST, in `page`; the lease-liveness `EXISTS` is
/// evaluated in `page_rows`, over the at-most-page_size rows that survive it.
/// The subquery used to sit in a CTE spanning the whole runner table, which
/// PostgreSQL answered by hashing the ENTIRE `runner_leases` table once per
/// request — 6 468 buffer hits against a 200 000-row lease table, against 75
/// for the page-scoped index lookups that replace it.
///
/// `total` keeps its meaning across the rewrite: `COUNT(*) OVER()` is a window
/// function, so it is computed over the full row set before `LIMIT` applies.
///
/// Two `{s}` slots, both the ORDER BY clause, and both fed from
/// `sortClauseFor`'s fixed allowlist — never from user input.
/// `$1` lease status, `$2` now_ms, `$3` limit, `$4` offset.
pub const SELECT_RUNNER_PAGE_FMT =
    \\WITH page AS (
    \\    SELECT r.id, r.host_id, r.sandbox_tier, r.admin_state, r.labels, r.last_seen_at, r.created_at,
    \\           COUNT(*) OVER()::bigint AS total,
    \\           ROW_NUMBER() OVER (ORDER BY {s})::bigint AS page_ord
    \\    FROM fleet.runners r
    \\    ORDER BY {s}
    \\    LIMIT $3 OFFSET $4
    \\),
    \\page_rows AS (
    \\    SELECT p.id::text, p.host_id, p.sandbox_tier, p.admin_state, p.labels::text, p.last_seen_at, p.created_at,
    \\           EXISTS (
    \\               SELECT 1
    \\               FROM fleet.runner_leases l
    \\               WHERE l.runner_id = p.id
    \\                 AND l.status = $1
    \\                 AND l.lease_expires_at > $2
    \\           ) AS has_live_lease,
    \\           p.total, false AS count_only, p.page_ord
    \\    FROM page p
    \\),
    \\total_row AS (
    \\    SELECT ''::text, ''::text, ''::text, 'active'::text, '[]'::text, 0::bigint, 0::bigint,
    \\           false, COUNT(*)::bigint, true, NULL::bigint
    \\    FROM fleet.runners
    \\    WHERE NOT EXISTS (SELECT 1 FROM page)
    \\)
    \\SELECT * FROM page_rows
    \\UNION ALL
    \\SELECT * FROM total_row
    \\ORDER BY count_only ASC, page_ord ASC NULLS LAST
;
