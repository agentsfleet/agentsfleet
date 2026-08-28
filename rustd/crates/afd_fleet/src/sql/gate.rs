//! The statement a recorded gate's durable answer is read through.
//!
//! Copied from `fleet_runtime/sql.zig`. One statement here today because one is
//! what the lease path reads: the gate ROW is written by the park, and the
//! resolve is the tenant plane's.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::gate::{Claim, Stated, Status};

/// The durable status of one approval gate.
///
/// `ORDER BY created_at DESC LIMIT 1` rather than a bare lookup, and it is not
/// defensive: `action_id` carries no unique constraint, so a re-raised gate for
/// the same action leaves more than one row and the NEWEST is the one a poll
/// must honour. Dropping the ordering would resolve against whichever row the
/// scan reached first.
///
/// `$1` action.
pub const SELECT_GATE_STATUS: &str = "\
SELECT status FROM core.fleet_approval_gates
WHERE action_id = $1
ORDER BY created_at DESC LIMIT 1";

/// Raise one approval gate, in the `pending` state a human answers out of.
///
/// Text from `fleet_runtime/sql.zig`'s `INSERT_GATE`, with the `::uuid` and
/// `::text` casts every other statement in this crate carries: the Zig driver
/// sends an untyped parameter and lets Postgres infer, sqlx binds `&str` as
/// `text`, and `id`/`fleet_id`/`workspace_id` are `UUID` columns. Column list,
/// column order and the two literal `''` columns are unchanged.
///
/// `resolved_by` and `detail` are literal `''` rather than binds because a
/// pending gate has no resolver and no resolution note — [`RESOLVE_GATE`]'s
/// job, and it is the tenant plane's, not this crate's. They are `NOT NULL`
/// columns, so the empty string is the row's own "not yet".
///
/// `$1` gate row, `$2` fleet, `$3` workspace, `$4` action, `$5` tool,
/// `$6` action name, `$7` kind, `$8` proposed action, `$9` evidence,
/// `$10` blast radius, `$11` deadline, `$12` status, `$13` now, `$14` event,
/// `$15` stated binding, `$16` spend count, `$17` spend ceiling.
pub const INSERT_GATE: &str = "\
INSERT INTO core.fleet_approval_gates
  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
   resolved_by, status, detail, created_at, event_id, stated_binding,
   spend_count, spend_ceiling)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9::jsonb, $10, $11, \
'', $12, '', $13, $14, $15::jsonb, $16, $17)";

/// Stop a fleet, so nothing else of its work is admitted.
///
/// Written by the two auto-kill paths — a tripped anomaly rule and an
/// `auto_kill` gate rule — and by nothing else in this crate. Inline in
/// `fleet/approval_gate.zig`'s `pauseFleet`; collected here because RULE SQLMOD
/// is what makes the verbatim-SQL parity review possible at all.
///
/// `$1` now, `$2` fleet.
pub const PAUSE_FLEET: &str = "\
UPDATE core.fleets SET status = 'paused', updated_at = $1 WHERE id = $2::uuid";

/// One pending gate, bound to [`INSERT_GATE`] in `$n` order.
///
/// The shape a high-arity statement takes in this crate (see
/// [`super::runner::RegisterRow`]): seventeen binds, most of them strings that
/// compile clean in any order, so the field names are what a reader checks
/// rather than a position they have to count to.
///
/// The two halves of the card arrive as the two TYPES that carry their
/// provenance, not as loose strings — which is what makes it impossible to bind
/// model prose into a column the workspace half owns.
#[derive(Debug, Clone, Copy)]
pub struct PendingRow<'a> {
    /// The gate row's own identifier.
    pub gate_id: &'a Uuid7,
    /// The fleet whose work is held.
    pub fleet_id: &'a Uuid7,
    /// The workspace it belongs to.
    pub workspace_id: &'a Uuid7,
    /// The action a human is asked about, and the reference's own identifier.
    pub action_id: &'a Uuid7,
    /// What the daemon and the workspace assert.
    pub stated: Stated<'a>,
    /// What the fleet's model claims — bounded and card-safe by construction.
    pub claim: &'a Claim,
    /// When the question lapses.
    pub deadline: UnixMillis,
    /// The event this question is about.
    pub event_id: &'a str,
    /// The approved reach, as the mint will read it back.
    pub stated_binding: Option<&'a str>,
    /// The spend counter's opening value, for a bounded approval.
    pub spend_count: Option<i64>,
    /// The instant the row is created at.
    pub now: UnixMillis,
}

impl<'a> PendingRow<'a> {
    /// Binds this row to [`INSERT_GATE`], in `$n` order.
    ///
    /// The status (`$12`) is supplied here rather than by a caller: every row
    /// this statement writes opens `pending`, and a caller able to pass another
    /// value could write a gate already resolved.
    pub fn bind(&'a self) -> super::runner::Bound<'a> {
        sqlx::query(INSERT_GATE)
            .bind(self.gate_id.as_str())
            .bind(self.fleet_id.as_str())
            .bind(self.workspace_id.as_str())
            .bind(self.action_id.as_str())
            .bind(self.stated.tool)
            .bind(self.stated.action)
            .bind(self.stated.kind)
            .bind(self.claim.proposed_action())
            .bind(self.claim.evidence())
            .bind(self.stated.radius)
            .bind(self.deadline.as_millis())
            .bind(Status::Pending.as_str())
            .bind(self.now.as_millis())
            .bind(self.event_id)
            .bind(self.stated_binding)
            .bind(self.spend_count)
            .bind(self.stated.spend_ceiling)
    }
}

/// The approved repository-write gate a lease's repair branch is named from.
///
/// Copied from `fleet_runtime/sql.zig`. Every predicate past the first three
/// exists to refuse a row that is approved but not USABLE, and each names a
/// different way that happens:
///
/// - `updated_at IS NOT NULL AND updated_at <= timeout_at` — the answer
///   arrived, and it arrived before the question lapsed. An approval recorded
///   after the deadline is a human answering a gate that had already expired.
/// - `stated_binding IS NOT NULL` — the reach was recorded. Without it there
///   is nothing to compare the fleet's current config against, and the caller
///   would have to decide what an unrecorded reach authorises. It authorises
///   nothing, and this is where that is enforced.
/// - `spend_count IS NOT NULL AND spend_ceiling = $5` — the row was raised
///   with THIS build's ceiling. A gate approved under a different ceiling was
///   approved for a different blast radius.
///
/// `ORDER BY created_at DESC, id DESC` — newest first, and `id` breaks a tie
/// so the answer is deterministic when two gates share an instant. `LIMIT 1`
/// after that ordering makes this the most recent usable approval, not an
/// arbitrary one.
///
/// The binding comparison itself is NOT in the statement. It is
/// `RepositoryBinding::matches_recorded`, in Rust, because set equality that
/// is case-insensitive and order-insensitive in both directions is not
/// something to express in SQL and then have to keep in agreement with the
/// serializer.
///
/// `$1` fleet, `$2` event, `$3` gate kind, `$4` approved status, `$5` ceiling.
pub const SELECT_APPROVED_WRITE_GATE: &str = "\
SELECT id::text, stated_binding::text FROM core.fleet_approval_gates
WHERE fleet_id = $1::uuid AND event_id = $2 AND gate_kind = $3
  AND status = $4 AND updated_at IS NOT NULL AND updated_at <= timeout_at
  AND stated_binding IS NOT NULL
  AND spend_count IS NOT NULL AND spend_ceiling = $5
ORDER BY created_at DESC, id DESC
LIMIT 1";

/// The write gate a mint spends from, locked for the spend.
///
/// Text from `http/handlers/runner/sql.zig`'s `SELECT_WRITE_GATE_FOR_MINT`.
/// Deliberately NOT [`SELECT_APPROVED_WRITE_GATE`], and the difference is the
/// point: that statement answers "may this lease author a branch" and folds
/// every refusal into no row, because its caller has one thing to say. This one
/// answers a runner that must be TOLD which refusal it met — an unapproved
/// gate, a reach that drifted, and an exhausted allowance are three different
/// remedies and three registry codes — so the row comes back whatever its
/// state and the verdict is decided in Rust.
///
/// `FOR UPDATE` is what makes the spend atomic: the row is held from the read
/// until the update commits, so two concurrent mints on one approval cannot
/// both see the same `spend_count`.
///
/// The kind is a `WHERE` clause rather than a check afterwards, because gates
/// of other kinds share the event id — an install-time grant card raised after
/// the write card would otherwise become "the newest gate" and shadow an answer
/// a human already gave. `id DESC` settles a same-millisecond tie.
///
/// `$1` fleet, `$2` event, `$3` gate kind.
pub const LOCK_WRITE_GATE_FOR_MINT: &str = "\
SELECT id::text, status, stated_binding::text, timeout_at, updated_at,
       spend_count, spend_ceiling
FROM core.fleet_approval_gates
WHERE fleet_id = $1::uuid AND event_id = $2 AND gate_kind = $3
ORDER BY created_at DESC, id DESC
LIMIT 1
FOR UPDATE";

/// Spends one request against an approved write gate.
///
/// The predicates are the same conditions the read already checked, restated
/// where the WRITE happens: a row that changed between the two — answered
/// again, or spent by a mint that got there first — updates nothing, and zero
/// affected rows is the exhausted answer. The check and the spend are one
/// decision even though they are two statements.
///
/// `$1` gate, `$2` approved status.
pub const SPEND_WRITE_GATE_FOR_MINT: &str = "\
UPDATE core.fleet_approval_gates
SET spend_count = spend_count + 1
WHERE id = $1::uuid AND status = $2
  AND spend_count IS NOT NULL AND spend_ceiling IS NOT NULL
  AND spend_count < spend_ceiling";
