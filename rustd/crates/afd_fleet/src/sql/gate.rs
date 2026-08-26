//! The statement a recorded gate's durable answer is read through.
//!
//! Copied from `fleet_runtime/sql.zig`. One statement here today because one is
//! what the lease path reads: the gate ROW is written by the park, and the
//! resolve is the tenant plane's.

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
