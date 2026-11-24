//! The statement a raised gate is written through.
//!
//! A gate is PENDING while `resolved_at` is NULL — the schema's one marker
//! (`afd_db/schema/20261108122600_fleet_approval_gates.sql`) — and every
//! instant on it is the database's: the row is stamped `now()`, and its
//! deadline, which a card shows and nothing enforces, is `now()` plus the
//! caller's window. The resolve is the tenant plane's.

/// Raise one approval gate, in the `pending` state a human answers out of,
/// and answer how many of the fleet's gates now wait — this one included.
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
/// `$10` blast radius, `$11` deadline window, `$12` status, `$13` turn.
pub const INSERT_GATE: &str = include_str!("insert_gate.sql");
