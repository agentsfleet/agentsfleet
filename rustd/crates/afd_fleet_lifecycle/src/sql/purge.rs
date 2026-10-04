//! The statements a hard purge runs on `core`.
//!
//! Separated from [`crate::sql`] because they only make sense read together.
//! The memory rows are not here: they cascade from the fleet row (schema/820).

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

/// The child rows no foreign key cascades, deleted before the parent.
///
/// `core.fleet_events` and `core.integration_grants` are absent because both
/// are `ON DELETE CASCADE`. `billing.usage_ledger` is absent for a different
/// reason: nothing there references the fleet any more — schema/915 dropped
/// that foreign key so a charge the wallet was already debited for outlives
/// the fleet still naming which one it paid for. Erasing one would falsify the
/// reconciliation between the two, and no role here holds `DELETE` on that
/// table anyway.
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
    use super::PURGE_CHILDREN;

    /// The regression this file exists for: a memory delete sitting in the slice
    /// that runs as `api_runtime` is exactly the shape that shipped broken.
    #[test]
    fn should_keep_memory_out_of_the_directly_deleted_children() {
        for statement in PURGE_CHILDREN {
            assert!(
                !statement.contains("memory."),
                "a memory statement in PURGE_CHILDREN runs without the role: {statement}"
            );
        }
    }
}
