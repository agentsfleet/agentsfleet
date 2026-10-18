//! The statements a raised gate is written and read through.
//!
//! The lease path reads one statement here, [`SELECT_GATE_STATUS`]: the gate
//! ROW is written by the park, and the resolve is the tenant plane's.
//!
//! A gate is PENDING while `resolved_at` is NULL — the schema's one marker
//! (`schema/1730_fleet_approval_gates.sql`) — and every instant on it is the
//! database's: the row is stamped `now()`, its deadline is `now()` plus the
//! rule's timeout, and whether that deadline has passed is read against
//! `statement_timestamp()`.

use std::time::Duration;

use afd_api_wire::approval::GateStatus;
use afd_core::id::Uuid7;
use sqlx::postgres::PgArguments;
use sqlx::query::QueryScalar;

use crate::gate::{Claim, Stated};

/// The durable status of one approval gate, and whether its deadline has
/// passed on the database clock.
///
/// `ORDER BY created_at DESC, id DESC LIMIT 1` rather than a bare lookup, and
/// it is not defensive: `action_id` carries no unique constraint, so a
/// re-raised gate for the same action leaves more than one row and the NEWEST
/// is the one a poll must honour. Dropping the ordering would resolve against
/// whichever row the scan reached first; the `id` tie-break orders two raised
/// in one millisecond by the order they were minted.
///
/// The lapse rides the same read because a pending gate is the one a poll
/// reaches this statement for — its answer is not in the mirror yet — so the
/// deadline comparison costs no round trip of its own and reads the clock that
/// stamped the deadline.
///
/// `$1` action.
pub const SELECT_GATE_STATUS: &str = include_str!("select_gate_status.sql");

/// Raise one approval gate, in the `pending` state a human answers out of,
/// and answer how many of the fleet's gates now wait — this one included.
///
/// The `::uuid` casts are the ones every other statement in this crate
/// carries: sqlx binds `&str` as `text`, and `id`/`fleet_id`/`workspace_id`
/// are `UUID` columns.
///
/// `detail` is a literal `''` rather than a bind because a pending gate has no
/// resolution note — the resolve's job, and it is the tenant plane's, not this
/// crate's — and the column is `NOT NULL`. `resolved_at`, `resolved_by` and
/// `resolved_by_name` are left NULL: that is what pending means.
///
/// The count rides the same statement so the frame announcing the gate
/// carries it without a second round trip. A data-modifying CTE and the
/// select after it run on ONE snapshot, so the select cannot see the row the
/// CTE inserts (PostgreSQL, "Data-Modifying Statements in WITH"); the union
/// with `raised` adds it back, which is what makes the answer "the fleet's
/// pending gates plus this one" rather than "plus one".
///
/// `$1` gate row, `$2` fleet, `$3` workspace, `$4` action, `$5` tool,
/// `$6` action name, `$7` type, `$8` proposed action, `$9` evidence,
/// `$10` blast radius, `$11` timeout, `$12` status, `$13` event,
/// `$14` stated binding, `$15` spend count, `$16` spend ceiling.
pub const INSERT_GATE: &str = include_str!("insert_gate.sql");

/// Stop a fleet, so nothing else of its work is admitted.
///
/// Written by the two auto-kill paths — a tripped anomaly rule and an
/// `auto_kill` gate rule — and by nothing else in this crate. Kept here with
/// the crate's other statements, per RULE SQLMOD, so every statement it runs
/// can be reviewed in one place.
///
/// `$1` the paused status, `$2` fleet.
pub const PAUSE_FLEET: &str = include_str!("pause_fleet.sql");

/// One pending gate, bound to [`INSERT_GATE`] in `$n` order.
///
/// The shape a high-arity statement takes in this workspace (see
/// `afd_runner`'s `RegisterRow`): sixteen binds, most of them strings that
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
    /// What the daemon and the workspace assert, the timeout included.
    pub stated: Stated<'a>,
    /// What the fleet's model claims — bounded and card-safe by construction.
    pub claim: &'a Claim,
    /// The event this question is about.
    pub event_id: &'a str,
    /// The approved reach, as the mint will read it back.
    pub stated_binding: Option<&'a str>,
}

impl<'a> PendingRow<'a> {
    /// Binds this row to [`INSERT_GATE`], in `$n` order, as the one count the
    /// statement answers.
    ///
    /// The status (`$12`) is supplied here rather than by a caller: every row
    /// this statement writes opens `pending`, and a caller able to pass another
    /// value could write a gate already resolved.
    pub fn bind(&'a self) -> QueryScalar<'a, sqlx::Postgres, i64, PgArguments> {
        sqlx::query_scalar(INSERT_GATE)
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
            .bind(timeout(self.stated.timeout))
            .bind(GateStatus::Pending.as_str())
            .bind(self.event_id)
            .bind(self.stated_binding)
            // Both NULL, always. No gate funds credential requests any more:
            // the standing grant authorises the mint and `budget.daily_nanos`
            // bounds the money. The columns stay for the spend a resolved gate
            // may still record, which the schema's guard trigger polices.
            .bind(None::<i64>)
            .bind(None::<i64>)
    }
}

/// A rule's timeout as the interval Postgres adds to its clock.
///
/// Floored to whole milliseconds: the deadline column holds milliseconds
/// (`TIMESTAMPTZ(3)`), and sqlx refuses to encode a span finer than a
/// microsecond, so a timeout carrying sub-millisecond noise would otherwise
/// fail the park rather than round.
fn timeout(span: Duration) -> Duration {
    Duration::from_millis(u64::try_from(span.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::timeout;

    #[test]
    fn a_timeout_binds_in_whole_milliseconds() {
        assert_eq!(timeout(Duration::from_secs(90)), Duration::from_secs(90));
        // A millisecond and a half keeps its whole millisecond and drops the
        // rest, rather than failing the encode.
        let ragged = Duration::from_millis(1).saturating_add(Duration::from_micros(500));
        assert_eq!(timeout(ragged), Duration::from_millis(1));
        assert_eq!(timeout(Duration::ZERO), Duration::ZERO);
    }
}
