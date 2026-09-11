//! The statements a hard purge runs, and the two role changes it runs them under.
//!
//! Separated from [`crate::sql`] because the purge is the one path in this crate
//! that crosses a schema boundary `api_runtime` cannot cross on its own, and the
//! statements that carry it across only make sense read together.

/// Tells the append-only trigger that THIS transaction is a real purge.
///
/// `core.fleet_approval_gates` refuses a DELETE outright unless
/// `fleet.allow_gate_purge` is `on` (schema/810). A hard purge is the one caller
/// entitled to say so, and `SET LOCAL` keeps the entitlement inside the
/// transaction that earned it rather than leaving it on a pooled connection for
/// whatever runs next.
///
/// Not parameterised, because `SET LOCAL` takes no bind parameters — the value
/// is a literal the trigger compares against, and it is the only literal here.
pub(crate) const ALLOW_GATE_PURGE: &str = "SET LOCAL fleet.allow_gate_purge = 'on'";

/// Take the role that may reach memory, for this transaction only.
///
/// Same statement and same reasoning as `afd_fleet`'s `ASSUME_MEMORY_ROLE`, and
/// deliberately a second copy rather than a dependency: this crate does not
/// otherwise know `afd_fleet`, and one `const` in each crate is cheaper than a
/// crate edge drawn to share nine words of SQL. [`RELEASE_ROLE`] documents the
/// pairing that keeps them honest.
///
/// `api_runtime` holds `memory_runtime` WITH INHERIT FALSE (schema/110), so the
/// privilege exists only while it is assumed. The daemon's login role reaches it
/// transitively — login → `api_runtime` → `memory_runtime`, every hop SET TRUE.
pub(crate) const ASSUME_MEMORY_ROLE: &str = "SET LOCAL ROLE memory_runtime";

/// Give the memory role back before the rest of the purge.
///
/// `NONE` rather than `RESET ROLE`: both return to the session role, and only
/// this one says so in the same `SET LOCAL` vocabulary it is undoing.
///
/// Load-bearing, not tidiness. `SET LOCAL ROLE` holds until the transaction
/// ends, and `memory_runtime` reaches memory and nothing else — its whole point.
/// Leaving it on would refuse every `core` delete that follows, turning one
/// permission error into a different one.
pub(crate) const RELEASE_ROLE: &str = "SET LOCAL ROLE NONE";

/// The memory rows, deleted under [`ASSUME_MEMORY_ROLE`].
///
/// Alone rather than in [`PURGE_CHILDREN`] because it is the only child whose
/// deletion needs a different role, and a slice cannot say that about one of its
/// elements. Keeping it in that slice is what let it run as `api_runtime` for
/// fifteen days.
///
/// `$1` fleet.
pub(crate) const PURGE_MEMORY: &str = "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid";

/// The child rows no foreign key cascades, deleted before the parent.
///
/// `core.fleet_events` and `core.integration_grants` are absent because both
/// are `ON DELETE CASCADE`. `billing.usage_ledger` is absent for a different
/// reason: its `fleet_id` is `ON DELETE SET NULL`, so a charge the wallet was
/// already debited for outlives the fleet with its tenant scope intact.
/// Erasing one would falsify the reconciliation between the two, and no role
/// here holds `DELETE` on that table anyway.
///
/// A slice rather than two named constants: the purge runs them in order inside
/// one transaction and never reaches for an individual one, so naming each would
/// be two symbols with no call site.
///
/// Both need a DELETE grant that schema/510 and schema/810 did not make;
/// schema/900 makes it.
///
/// `$1` fleet.
pub(crate) const PURGE_CHILDREN: &[&str] = &[
    "DELETE FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
    "DELETE FROM core.fleet_sessions WHERE fleet_id = $1::uuid",
];

#[cfg(test)]
mod tests {
    use super::{ASSUME_MEMORY_ROLE, PURGE_CHILDREN, PURGE_MEMORY, RELEASE_ROLE};

    /// The pairing is the whole safety property, so it is asserted rather than
    /// trusted to a reader noticing both constants.
    #[test]
    fn should_release_the_role_it_assumes() {
        assert!(ASSUME_MEMORY_ROLE.starts_with("SET LOCAL ROLE "));
        assert!(RELEASE_ROLE.starts_with("SET LOCAL ROLE "));
        assert_ne!(ASSUME_MEMORY_ROLE, RELEASE_ROLE);
    }

    /// The regression this file exists for: a memory delete sitting in the slice
    /// that runs as `api_runtime` is exactly the shape that shipped broken.
    #[test]
    fn should_keep_memory_out_of_the_directly_deleted_children() {
        assert!(PURGE_MEMORY.contains("memory."));
        for statement in PURGE_CHILDREN {
            assert!(
                !statement.contains("memory."),
                "a memory statement in PURGE_CHILDREN runs without the role: {statement}"
            );
        }
    }
}
