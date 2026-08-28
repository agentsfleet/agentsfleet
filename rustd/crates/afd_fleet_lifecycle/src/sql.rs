//! Every statement this crate runs, and nothing else.
//!
//! Ports of `http/handlers/fleets/sql.zig`, plus the two install-source reads
//! from `fleet_library/sql.zig`. What they SELECT and what they predicate on is
//! the original's; the only reshaping is the string form this workspace writes
//! statements in.
//!
//! # Every statement carries the workspace in its predicate
//!
//! Not one of them trusts the handler to have checked first. A fleet id
//! belonging to another workspace resolves NO ROW rather than the wrong row,
//! which is what makes the 404 in front of it honest: existence is never
//! disclosed, because nothing here can answer about a row the caller does not
//! own. The ownership LAYER is a capability question and this is the tenancy
//! boundary; the two are independent on purpose, and a fault in either alone
//! still leaves the other standing.
//!
//! # The counters are joined, never re-aggregated
//!
//! `events_processed` and `budget_used_nanos` come from the one-to-one
//! `core.fleet_activity_counters` row migration-030's triggers maintain. A list
//! page therefore costs one index scan and one join, not a per-row subselect
//! over `core.fleet_events` — which is what a page of fifty fleets would
//! otherwise charge every dashboard load.

/// The columns a list page reads, in the order [`crate::read`] indexes them.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!`
/// takes literals only and the two page statements must not carry two
/// hand-kept copies of eight column expressions. A column added to one spelling
/// and not the other would not fail to compile — it would shift every field
/// after it and mis-read the row (RULE UFS).
macro_rules! page_columns {
    () => {
        "SELECT f.id::text, f.name, f.status, f.created_at, f.updated_at, \
                (f.config_json->'x-agentsfleet'->'triggers')::text, \
                COALESCE(c.events_processed, 0), COALESCE(c.budget_used_nanos, 0) \
         FROM core.fleets f \
         LEFT JOIN core.fleet_activity_counters c ON c.fleet_id = f.id "
    };
}

/// The first page of a workspace's fleets, newest first.
///
/// `$1` workspace · `$2` how many rows to fetch. The caller asks for ONE more
/// row than the page it will serve, so whether a next page exists is a fact
/// about the walk rather than a guess from a full page.
pub(crate) const SELECT_FLEET_PAGE_FIRST: &str = concat!(
    page_columns!(),
    "WHERE f.workspace_id = $1::uuid \
     ORDER BY f.created_at DESC, f.id DESC \
     LIMIT $2"
);

/// A later page, resumed from the boundary the last one ended on.
///
/// `$1` workspace · `$2` boundary instant · `$3` boundary id · `$4` fetch count.
///
/// The tuple comparison is spelled out rather than written as a row constructor
/// because `f.id` is compared as TEXT: the cursor carries the identifier's
/// canonical spelling, and a `uuid` comparison would order by byte layout
/// instead — a different walk for two fleets minted in the same millisecond,
/// which is exactly the case the composite key exists to get right.
pub(crate) const SELECT_FLEET_PAGE_AFTER: &str = concat!(
    page_columns!(),
    "WHERE f.workspace_id = $1::uuid \
       AND (f.created_at < $2 OR (f.created_at = $2 AND f.id::text < $3)) \
     ORDER BY f.created_at DESC, f.id DESC \
     LIMIT $4"
);

/// One fleet, whole — a page row's fields plus the editable markdown pair.
///
/// `$1` fleet · `$2` workspace. The extra columns over a page row are what the
/// source editor needs and a list does not: the authored `SKILL.md`, the
/// `TRIGGER.md` beside it, and the bundle pin a runner materialises from.
pub(crate) const SELECT_FLEET_DETAIL: &str = "\
SELECT f.id::text, f.name, f.status, f.source_markdown, f.trigger_markdown, \
       f.bundle_content_hash, \
       (f.config_json->'x-agentsfleet'->'triggers')::text, \
       COALESCE(c.events_processed, 0), COALESCE(c.budget_used_nanos, 0), \
       f.created_at, f.updated_at \
FROM core.fleets f \
LEFT JOIN core.fleet_activity_counters c ON c.fleet_id = f.id \
WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid";

/// Writes the row, taking the tenant from the workspace rather than the caller.
///
/// `$1` fleet · `$2` workspace · `$3` name · `$4` source · `$5` trigger ·
/// `$6` config · `$7` status · `$8` required tags · `$9` bundle hash ·
/// `$10` snapshot key · `$11` the instant, stamped on both timestamps.
///
/// `INSERT … SELECT FROM core.workspaces` and not `VALUES`: the tenant column
/// is READ from the workspace being installed into, so a request cannot name a
/// tenant at all. A workspace that does not exist writes zero rows and raises
/// nothing, which the caller reads as the install finding no workspace.
pub(crate) const INSERT_FLEET: &str = "\
INSERT INTO core.fleets \
  (id, workspace_id, tenant_id, name, source_markdown, trigger_markdown, \
   config_json, status, required_tags, bundle_content_hash, \
   bundle_snapshot_key, created_at, updated_at) \
SELECT $1::uuid, w.id, w.tenant_id, $3, $4, $5, $6::jsonb, $7, $8::text[], \
       $9, $10, $11, $11 \
FROM core.workspaces w WHERE w.id = $2::uuid";

/// Moves a fleet between two statuses, and only from the one named.
///
/// `$1` the status to set · `$2` the instant · `$3` fleet · `$4` workspace ·
/// `$5` the status the row must currently hold.
///
/// Guarded on the CURRENT status so a concurrent operator action is never
/// clobbered: an install finishing at the same instant somebody kills the fleet
/// writes zero rows rather than resurrecting it.
pub(crate) const UPDATE_FLEET_STATUS: &str = "\
UPDATE core.fleets SET status = $1, updated_at = $2 \
WHERE id = $3::uuid AND workspace_id = $4::uuid AND status = $5";

/// Removes a row an install could not finish setting up.
///
/// `$1` fleet · `$2` workspace. Workspace-scoped like everything else here, so
/// a rollback cannot reach across tenants even when handed a wrong identifier.
pub(crate) const DELETE_FLEET: &str = "\
DELETE FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid";

/// Every fleet identifier in one workspace.
///
/// `$1` workspace. No ORDER BY: the caller collects into a `BTreeSet`, which
/// is what makes the order stable, and asking Postgres to sort a set the caller
/// re-sorts anyway would be paying twice.
pub(crate) const SELECT_FLEET_IDS: &str = "\
SELECT id::text FROM core.fleets WHERE workspace_id = $1::uuid";

/// The status alone, for the purge's pre-flight and the steer's ingress check.
///
/// `$1` fleet · `$2` workspace.
pub(crate) const SELECT_FLEET_STATUS: &str = "\
SELECT status FROM core.fleets \
WHERE id = $1::uuid AND workspace_id = $2::uuid \
LIMIT 1";

/// Deletes only from the expected status, reporting whether it happened.
///
/// `$1` fleet · `$2` workspace · `$3` the status the row must hold.
///
/// `RETURNING id` is what lets the caller tell "already gone" from "still
/// running, refused" without a second read, and it closes the window between
/// the pre-flight probe and this statement where a concurrent PATCH could have
/// brought the fleet back.
pub(crate) const DELETE_FLEET_IN_STATUS: &str = "\
DELETE FROM core.fleets \
WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3 \
RETURNING id";

/// The child rows no foreign key cascades, deleted before the parent.
///
/// `core.fleet_events` and `core.integration_grants` are absent because both
/// are `ON DELETE CASCADE`. `billing.usage_ledger` is absent for a different
/// reason: its `fleet_id` is `ON DELETE SET NULL`, so a charge the wallet was
/// already debited for outlives the fleet with its tenant scope intact.
/// Erasing one would falsify the reconciliation between the two, and no role
/// here holds `DELETE` on that table anyway.
///
/// A slice rather than three named constants: the purge runs them in order
/// inside one transaction and never reaches for an individual one, so naming
/// each would be three symbols with no call site.
pub(crate) const PURGE_CHILDREN: &[&str] = &[
    "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid",
    "DELETE FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
    "DELETE FROM core.fleet_sessions WHERE fleet_id = $1::uuid",
];

/// The editable surface a PATCH reads before it writes.
///
/// `$1` fleet · `$2` workspace.
///
/// No `FOR UPDATE`, and that is the whole design. `patch_txn.zig` locks the row
/// for the duration of a read-modify-write; here the compare-and-set lives in
/// [`PATCH_FLEET`]'s own predicate, so this read needs no lock to be safe — a
/// concurrent write simply makes the UPDATE match no row. What that removes is
/// a transaction, three `SET LOCAL` timeouts, a `55P03` classification, and a
/// row lock held across a YAML reparse on every conditional save.
pub(crate) const SELECT_FLEET_EDITABLE: &str = "\
SELECT name, status, source_markdown, trigger_markdown FROM core.fleets \
WHERE id = $1::uuid AND workspace_id = $2::uuid";

/// Applies a PATCH, with the status machine expressed as the row predicate.
///
/// `$1` config · `$2` requested status · `$3` the instant · `$4` fleet ·
/// `$5` workspace · `$6` killed · `$7` stopped · `$8` active · `$9` the
/// statuses `stopped` may be reached from · `$10` the statuses `active` may be
/// reached from · `$11` trigger markdown · `$12` source markdown · `$13` name ·
/// `$14` required tags · `$15` the source digest the caller read, or NULL ·
/// `$16` the trigger digest they read, NULL where the column is.
///
/// `COALESCE` per column makes every field independently optional: an absent
/// field is untouched rather than nulled. The trailing disjunction is the
/// machine — a status change is accepted only when it is a no-op, or a
/// transition whose SOURCE status is in the allowed set for that target. An
/// illegal transition matches no row and returns nothing, so no caller can talk
/// the handler into one and no handler can forget to ask.
///
/// The machine lives HERE, in the row predicate, rather than in a read followed
/// by a decision followed by a write. That is not a style preference: the read
/// and the write would be two statements, and between them another request can
/// commit. As a predicate it is evaluated by the UPDATE itself, so the check
/// cannot go stale between being made and being acted on.
///
/// # The conditional guard is over exactly what the `ETag` hashes
///
/// `$15`/`$16` carry the digests of the two markdown columns as the caller last
/// read them, and a write proceeds only if both still match. That is the
/// compare-and-set an `If-Match` asks for, done atomically by the UPDATE — no
/// row lock, and no `updated_at` comparison, which would carry a real ABA hole:
/// the column is epoch MILLISECONDS, so two commits inside one millisecond
/// return it to a value a third reader is still holding. Postgres would not
/// close that with `TIMESTAMPTZ` either, because `now()` is transaction-START
/// time and two concurrent transactions read it identical.
///
/// Guarding on the SAME columns the tag hashes — rather than on the whole row —
/// is what keeps a concurrent status change from refusing an editor whose source
/// nobody touched. `xmin` would be exact and would get that wrong.
///
/// Digests rather than the values: the columns run to 200KB each, and 64 hex
/// characters say the same thing. Postgres hashes its own copy, so nothing is
/// resent.
pub(crate) const PATCH_FLEET: &str = "\
UPDATE core.fleets SET \
    config_json      = COALESCE($1::jsonb, config_json), \
    status           = COALESCE($2,        status), \
    trigger_markdown = COALESCE($11,       trigger_markdown), \
    source_markdown  = COALESCE($12,       source_markdown), \
    name             = COALESCE($13,       name), \
    required_tags    = COALESCE($14::text[], required_tags), \
    updated_at       = $3 \
WHERE id = $4::uuid \
  AND workspace_id = $5::uuid \
  AND status != $6 \
  AND ( \
        $2::text IS NULL \
     OR ($2 = $6) \
     OR ($2 = $7 AND status = ANY($9::text[])) \
     OR ($2 = $8 AND status = ANY($10::text[])) \
  ) \
  AND ($15::text IS NULL OR ( \
        encode(sha256(convert_to(source_markdown, 'UTF8')), 'hex') = $15 \
    AND encode(sha256(convert_to(trigger_markdown, 'UTF8')), 'hex') \
        IS NOT DISTINCT FROM $16 \
  )) \
RETURNING updated_at";

/// A platform library entry, resolved for install by its slug.
///
/// `$1` the entry's id · `$2` the visibility a published row carries.
///
/// Only a PUBLISHED row holding a bundle is installable. A draft resolves
/// nothing, so an unpublished fleet cannot be installed by anybody who merely
/// knows its identifier — the predicate is the check, rather than a handler
/// remembering to make one.
pub(crate) const SELECT_PLATFORM_INSTALL: &str = "\
SELECT skill_markdown, trigger_markdown, content_hash \
FROM core.fleet_library \
WHERE id = $1 AND visibility = $2 \
  AND content_hash IS NOT NULL AND skill_markdown IS NOT NULL";

/// A tenant library entry, resolved for install and scoped to its workspace.
///
/// `$1` the entry's id · `$2` workspace. An entry another workspace owns is
/// invisible here rather than forbidden, for the reason every statement in this
/// file scopes: a refusal that told the two apart would disclose the entry.
pub(crate) const SELECT_TENANT_INSTALL: &str = "\
SELECT skill_markdown, trigger_markdown, content_hash \
FROM core.tenant_fleet_library \
WHERE id = $1::uuid AND workspace_id = $2::uuid";

#[cfg(test)]
mod tests {
    use super::{SELECT_FLEET_PAGE_AFTER, SELECT_FLEET_PAGE_FIRST};

    #[test]
    fn both_page_statements_read_the_same_columns_in_the_same_order() {
        // The whole reason `page_columns!` is a macro. Both readers index the
        // result positionally, so a column that reached one statement and not
        // the other would not fail to compile — it would shift every field
        // after it and quietly mis-read the row.
        let columns = |statement: &str| {
            statement
                .split_once("WHERE")
                .map(|(head, _)| head.to_owned())
                .unwrap_or_default()
        };

        assert_eq!(
            columns(SELECT_FLEET_PAGE_FIRST),
            columns(SELECT_FLEET_PAGE_AFTER)
        );
        assert!(columns(SELECT_FLEET_PAGE_FIRST).contains("events_processed"));
    }

    #[test]
    fn every_page_statement_is_scoped_to_one_workspace() {
        // The tenancy boundary is the predicate, not the handler. A statement
        // that lost its scope would read another tenant's fleets with nothing
        // in front of it failing.
        for statement in [SELECT_FLEET_PAGE_FIRST, SELECT_FLEET_PAGE_AFTER] {
            assert!(
                statement.contains("f.workspace_id = $1::uuid"),
                "a page walk must never be able to leave its workspace"
            );
        }
    }
}
