//! The texts the read side runs, and the one column list they share.
//!
//! Split out of [`super`] because the statements are DATA — long, exact, and
//! read as a unit — while the methods beside them are control flow. Keeping
//! them here leaves each file a size a reviewer holds, and it puts the shared
//! column list next to the assertion that both statements expand from it.
//!
//! # Why the pieces are macros
//!
//! A `const` cannot be concatenated into another `const`, and the alternative —
//! writing the column list out twice — is the drift this exists to prevent.
//! Without it the same `EVENTS_SELECT` is repeated across eight concatenated
//! variants, with a ninth copy carrying two columns more.
//!
//! # The bodies are appended, not spliced
//!
//! On the wire, `request_json` and `response_text` sit between `status` and
//! `tokens` — that is the order the daemon already serves and a client already
//! reads. Here they go LAST, because SQL column order and JSON field order are
//! independent and only the second is a contract. Appending is what lets the
//! detail read decode the fifteen shared columns with the listing's own
//! decoder instead of a second copy of it.

/// The columns every read selects, in the order [`super::EventRow`] decodes.
///
/// `cost_nanos` is a correlated subselect rather than a `LEFT JOIN`: billing
/// writes up to two ledger rows per event — `receive` and `stage`, unique on
/// `(event_id, charge_type, fleet_id)` — so a join would duplicate the event row per leg
/// and a page of 50 would render as 100. The subselect keeps one row per event
/// and yields SQL NULL where no telemetry exists.
///
/// Two spellings of one column list: the history reads name the columns bare
/// over `core.fleet_events`, and a closing statement (`crate::sql`) names the
/// same fifteen off the alias its CTE hands to the select. The list is
/// written once so a column added to [`super::EventRow`] reaches every
/// statement that decodes one, or none.
macro_rules! shared_columns {
    () => {
        $crate::history::statement::shared_columns!(@ "", "core.fleet_events.")
    };
    ($alias:literal) => {
        $crate::history::statement::shared_columns!(@ concat!($alias, "."), concat!($alias, "."))
    };
    (@ $col:expr, $row:expr) => {
        concat!(
            "SELECT ", $col, "fleet_id::text, ", $col, "event_id, ", $col, "workspace_id::text, ",
            $col, "actor, ", $col, "event_type,\n       ",
            $col, "status, ", $col, "tokens, ", $col, "wall_ms,\n       ",
            $col, "failure_label, ", $col, "failure_detail, ", $col, "checkpoint_id, ",
            $col, "resumes_event_id,\n       ",
            $col, "created_at, ", $col, "updated_at,\n",
            "       (SELECT SUM(te.credit_deducted_nanos)::bigint\n",
            "          FROM billing.usage_ledger te\n",
            "         WHERE te.event_id = ", $row, "event_id\n",
            "           AND te.fleet_id = ", $row, "fleet_id) AS cost_nanos"
        )
    };
}
pub(crate) use shared_columns;

/// The two body columns, which only the detail read pays for.
///
/// `request_json` is JSONB in the table and is cast so sqlx hands back a
/// `String`; the alias is what makes the OUTPUT column's name a fact of this
/// text rather than of how PostgreSQL names a cast expression. `EventDetailRow`
/// reads these two by name — it is the only decoder in the workspace that does
/// — so an unaliased cast would make the read depend on `FigureColname`
/// recursing through the `TypeCast` node, and it would fail only against a live
/// Postgres, which the unit lane never runs.
macro_rules! body_columns {
    () => {
        ",
       request_json::text AS request_json, response_text"
    };
}

/// The table both statements read.
macro_rules! from_events {
    () => {
        "
FROM core.fleet_events
"
    };
}

/// The order every page walks and every cursor encodes (RULE KYS), then the
/// limit, bound at the placeholder numbered `$limit`.
macro_rules! newest_first {
    ($limit:literal) => {
        concat!(
            "
ORDER BY created_at DESC, event_id DESC
LIMIT $",
            $limit
        )
    };
}

/// The scope of a fleet's reads: `$1` workspace, `$2` fleet.
///
/// Both equalities stay in the row predicate, so a row is served only to the
/// workspace it belongs to. `schema/922` tells the planner the second implies
/// the first; without it their estimates multiply and a generic plan sorts.
macro_rules! fleet_scope {
    () => {
        "WHERE workspace_id = $1::uuid
  AND fleet_id = $2::uuid"
    };
}

/// The scope of a workspace's reads: `$1` workspace.
macro_rules! workspace_scope {
    () => {
        "WHERE workspace_id = $1::uuid"
    };
}

// The listing texts, one per scope, cursor and actor filter. A bound behind
// `IS NULL OR` cannot be decided by a generic plan, so nothing here is gated:
// the scope, cursor and `since` stay index conditions, and an absent actor
// filter is an absent predicate rather than a guard that drags the row
// estimate down until the planner sorts. Each text numbers its placeholders in
// the order `History` binds them: workspace, fleet when fleet-scoped, the
// cursor pair when resuming, the actor pattern when filtering, `since`, and
// the limit.

/// A fleet's first page: `$3` since, `$4` limit.
pub(super) const SELECT_FLEET_PAGE: &str = concat!(
    shared_columns!(),
    from_events!(),
    fleet_scope!(),
    "
  AND created_at >= $3",
    newest_first!(4)
);

/// A fleet's first page by actor: `$3` actor LIKE, `$4` since, `$5` limit.
pub(super) const SELECT_FLEET_PAGE_BY_ACTOR: &str = concat!(
    shared_columns!(),
    from_events!(),
    fleet_scope!(),
    "
  AND actor LIKE $3
  AND created_at >= $4",
    newest_first!(5)
);

/// A fleet's page after a cursor: `$3` cursor timestamp, `$4` cursor event
/// id, `$5` since, `$6` limit.
pub(super) const SELECT_FLEET_PAGE_AFTER: &str = concat!(
    shared_columns!(),
    from_events!(),
    fleet_scope!(),
    "
  AND (created_at, event_id) < ($3, $4)
  AND created_at >= $5",
    newest_first!(6)
);

/// A fleet's page by actor after a cursor: `$3` cursor timestamp, `$4` cursor
/// event id, `$5` actor LIKE, `$6` since, `$7` limit.
pub(super) const SELECT_FLEET_PAGE_AFTER_BY_ACTOR: &str = concat!(
    shared_columns!(),
    from_events!(),
    fleet_scope!(),
    "
  AND (created_at, event_id) < ($3, $4)
  AND actor LIKE $5
  AND created_at >= $6",
    newest_first!(7)
);

/// A workspace's first page: `$2` since, `$3` limit.
pub(super) const SELECT_WORKSPACE_PAGE: &str = concat!(
    shared_columns!(),
    from_events!(),
    workspace_scope!(),
    "
  AND created_at >= $2",
    newest_first!(3)
);

/// A workspace's first page by actor: `$2` actor LIKE, `$3` since, `$4` limit.
pub(super) const SELECT_WORKSPACE_PAGE_BY_ACTOR: &str = concat!(
    shared_columns!(),
    from_events!(),
    workspace_scope!(),
    "
  AND actor LIKE $2
  AND created_at >= $3",
    newest_first!(4)
);

/// A workspace's page after a cursor: `$2` cursor timestamp, `$3` cursor
/// event id, `$4` since, `$5` limit.
pub(super) const SELECT_WORKSPACE_PAGE_AFTER: &str = concat!(
    shared_columns!(),
    from_events!(),
    workspace_scope!(),
    "
  AND (created_at, event_id) < ($2, $3)
  AND created_at >= $4",
    newest_first!(5)
);

/// A workspace's page by actor after a cursor: `$2` cursor timestamp, `$3`
/// cursor event id, `$4` actor LIKE, `$5` since, `$6` limit.
pub(super) const SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR: &str = concat!(
    shared_columns!(),
    from_events!(),
    workspace_scope!(),
    "
  AND (created_at, event_id) < ($2, $3)
  AND actor LIKE $4
  AND created_at >= $5",
    newest_first!(6)
);

/// The listing text for a read's scope, cursor and actor filter — the same
/// three facts `History` decides its binds from, so the two cannot disagree
/// about which placeholders a text numbers.
pub(super) const fn listing_text(
    fleet_scoped: bool,
    resumes: bool,
    by_actor: bool,
) -> &'static str {
    match (fleet_scoped, resumes, by_actor) {
        (true, false, false) => SELECT_FLEET_PAGE,
        (true, false, true) => SELECT_FLEET_PAGE_BY_ACTOR,
        (true, true, false) => SELECT_FLEET_PAGE_AFTER,
        (true, true, true) => SELECT_FLEET_PAGE_AFTER_BY_ACTOR,
        (false, false, false) => SELECT_WORKSPACE_PAGE,
        (false, false, true) => SELECT_WORKSPACE_PAGE_BY_ACTOR,
        (false, true, false) => SELECT_WORKSPACE_PAGE_AFTER,
        (false, true, true) => SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR,
    }
}

/// One event by its identifier, scoped to the workspace and fleet that own it.
///
/// The scoping is in the STATEMENT rather than checked after the read: a row
/// belonging to another workspace must not come back and then be filtered, or
/// the filter becomes the only thing standing between two tenants.
///
/// Bounded by construction — the predicate names the table's whole primary key
/// beside the workspace, so this executes for exactly one event however much
/// history the fleet has.
///
/// `$1` workspace, `$2` fleet, `$3` event.
pub(super) const SELECT_DETAIL: &str = concat!(
    shared_columns!(),
    body_columns!(),
    from_events!(),
    "WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid AND event_id = $3"
);

// One fleet's chat thread, bodies included, keyset-paged newest-first: the
// third reader built from the same vocabulary, and the reason the bodies are a
// separate macro rather than spliced into one list. The thread pays for them
// exactly as the expanded read does, and the listing beside it does not.

/// A thread's first page: `$1` workspace, `$2` fleet, `$3` limit.
pub(super) const SELECT_THREAD_PAGE: &str = concat!(
    shared_columns!(),
    body_columns!(),
    from_events!(),
    fleet_scope!(),
    newest_first!(3)
);

/// A thread's page after a cursor: `$1` workspace, `$2` fleet, `$3` cursor
/// timestamp, `$4` cursor event id, `$5` limit.
pub(super) const SELECT_THREAD_PAGE_AFTER: &str = concat!(
    shared_columns!(),
    body_columns!(),
    from_events!(),
    fleet_scope!(),
    "
  AND (created_at, event_id) < ($3, $4)",
    newest_first!(5)
);

/// The waiting read's columns and scope: a fleet's steers on its queue that
/// no runner has yet.
macro_rules! queued_scope {
    () => {
        "\
SELECT fleet_id::text, workspace_id::text, actor, event_type, request_json, created_at, seq \
FROM core.fleet_admissions \
WHERE fleet_id = $2::uuid AND workspace_id = $1::uuid AND producer = $3 \
  AND receipt IS NOT NULL AND delivered_at IS NULL "
    };
}

/// The waiting read's order and cut, newest first.
macro_rules! queued_newest_first {
    ($limit:literal) => {
        concat!("ORDER BY created_at DESC, seq DESC LIMIT $", $limit)
    };
}

/// The fleet's waiting steers, newest first: `$1` workspace, `$2` fleet, `$3`
/// producer, `$4` limit.
///
/// Both partial indexes on `delivered_at IS NULL` (`schema/910`,
/// `schema/914`) key on `(fleet_id, created_at, seq)` and hold only in-flight
/// work, so whichever the planner takes, the fleet is an index condition, the
/// order comes off the index, and the cost follows what waits, never the
/// ledger's history. The logical id is spelled in Rust
/// (`afd_admission::logical_id`), so the two integers come back as they are.
pub(super) const SELECT_THREAD_QUEUED: &str = concat!(queued_scope!(), queued_newest_first!(4));

/// The same, older than a resumed page's cursor: `$4` `created_at`, `$5`
/// `event_id`, `$6` limit.
///
/// The cursor is a history key, `(created_at, event_id)`, and a waiting row's
/// event id is the logical id `{created_at}-{seq}`; spelling it here compares
/// the two on the one key the merged page is sorted and cut on.
pub(super) const SELECT_THREAD_QUEUED_AFTER: &str = concat!(
    queued_scope!(),
    "AND (created_at, created_at::text || '-' || seq::text) < ($4, $5) ",
    queued_newest_first!(6)
);

/// The waiting-messages texts, for the suite that asks Postgres how it plans
/// them.
#[cfg(feature = "test-util")]
pub const QUEUED_READ_TEXTS: [(&str, &str); 2] = [
    ("waiting", SELECT_THREAD_QUEUED),
    ("waiting after", SELECT_THREAD_QUEUED_AFTER),
];

/// Every listing and thread text, named, for the suite that asks Postgres how
/// it plans each one.
#[cfg(feature = "test-util")]
pub const READ_TEXTS: [(&str, &str); 10] = [
    ("fleet page", SELECT_FLEET_PAGE),
    ("fleet page by actor", SELECT_FLEET_PAGE_BY_ACTOR),
    ("fleet page after", SELECT_FLEET_PAGE_AFTER),
    (
        "fleet page after by actor",
        SELECT_FLEET_PAGE_AFTER_BY_ACTOR,
    ),
    ("workspace page", SELECT_WORKSPACE_PAGE),
    ("workspace page by actor", SELECT_WORKSPACE_PAGE_BY_ACTOR),
    ("workspace page after", SELECT_WORKSPACE_PAGE_AFTER),
    (
        "workspace page after by actor",
        SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR,
    ),
    ("thread page", SELECT_THREAD_PAGE),
    ("thread page after", SELECT_THREAD_PAGE_AFTER),
];

#[cfg(test)]
#[path = "statement/tests.rs"]
mod tests;
