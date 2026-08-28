//! The two texts the read side runs, and the one column list they share.
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
//! `fleet_events_store.zig` repeats its own `EVENTS_SELECT` across eight
//! concatenated variants for want of this, and `fleet_event_detail_store.zig`
//! carries a ninth copy with two columns added.
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
/// `(event_id, charge_type)` — so a join would duplicate the event row per leg
/// and a page of 50 would render as 100. The subselect keeps one row per event
/// and yields SQL NULL where no telemetry exists.
macro_rules! shared_columns {
    () => {
        "\
SELECT fleet_id::text, event_id, workspace_id::text, actor, event_type,
       status, tokens, wall_ms,
       failure_label, failure_detail, checkpoint_id, resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(te.credit_deducted_nanos)::bigint
          FROM billing.usage_ledger te
         WHERE te.event_id = core.fleet_events.event_id
           AND te.fleet_id = core.fleet_events.fleet_id) AS cost_nanos"
    };
}

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

/// The listing statement, shared by both entry points.
///
/// `$1` workspace, `$2` fleet or NULL, `$3` cursor timestamp or NULL,
/// `$4` cursor event id, `$5` actor LIKE or NULL, `$6` since or NULL,
/// `$7` limit.
pub(super) const SELECT_PAGE: &str = concat!(
    shared_columns!(),
    from_events!(),
    "WHERE workspace_id = $1::uuid
  AND ($2::text IS NULL OR fleet_id = $2::uuid)
  AND ($3::bigint IS NULL OR (created_at, event_id) < ($3, $4))
  AND ($5::text IS NULL OR actor LIKE $5)
  AND ($6::bigint IS NULL OR created_at >= $6)
ORDER BY created_at DESC, event_id DESC
LIMIT $7"
);

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

/// One fleet's chat thread, bodies included, keyset-paged newest-first.
///
/// The third statement built from the same vocabulary, and the reason the
/// bodies are a separate macro rather than spliced into one list: the thread
/// pays for them exactly as the expanded read does, and the listing beside
/// them still does not.
///
/// The cursor is NULL-gated the way [`SELECT_PAGE`]'s is, so the first page
/// and the resumed page are one text rather than the two
/// `fleet_event_detail_store.zig` carries — a fix applied to one of two is the
/// failure mode that shape invites.
///
/// `$1` workspace, `$2` fleet, `$3` cursor timestamp or NULL, `$4` cursor
/// event id, `$5` limit.
pub(super) const SELECT_THREAD_PAGE: &str = concat!(
    shared_columns!(),
    body_columns!(),
    from_events!(),
    "WHERE workspace_id = $1::uuid
  AND fleet_id = $2::uuid
  AND ($3::bigint IS NULL OR (created_at, event_id) < ($3, $4))
ORDER BY created_at DESC, event_id DESC
LIMIT $5"
);

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::*;

    #[test]
    fn both_statements_share_one_column_list() {
        // The macro is the single source; this pins that both statements
        // actually expand from it rather than carrying a hand-copied prefix.
        assert!(SELECT_PAGE.starts_with(shared_columns!()));
        assert!(SELECT_DETAIL.starts_with(shared_columns!()));
        assert!(SELECT_THREAD_PAGE.starts_with(shared_columns!()));
    }

    #[test]
    fn only_the_detail_read_pays_for_the_bodies() {
        // The whole reason two statements exist rather than one: a page of up
        // to two hundred rows must not carry a trigger payload and an agent's
        // full answer per row.
        assert!(SELECT_DETAIL.contains(body_columns!()));
        // The thread pays for them too — it IS the expanded read, paged.
        assert!(SELECT_THREAD_PAGE.contains(body_columns!()));
        assert!(!SELECT_PAGE.contains("request_json"));
        assert!(!SELECT_PAGE.contains("response_text"));
    }

    #[test]
    fn the_bodies_follow_every_shared_column() {
        // `EventDetailRow` decodes the shared columns with the LISTING's
        // decoder, which reads by index. That only holds while the bodies come
        // after all fifteen of them — so the ordering is an invariant of the
        // text, not a convention of how it was written.
        for text in [SELECT_DETAIL, SELECT_THREAD_PAGE] {
            let bodies = text
                .find("request_json")
                .expect("a bodies-included read selects the trigger payload");
            let last_shared = text
                .find("cost_nanos")
                .expect("every read selects the summed cost");
            assert!(bodies > last_shared);
        }
    }
}
