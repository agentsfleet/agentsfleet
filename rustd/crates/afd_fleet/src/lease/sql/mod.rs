//! Every statement the lease plane runs.
//!
//! One `sql` module per crate-to-be rather than one for the whole of
//! `afd_fleet`: RULE SQLMOD puts a statement with the code that runs it, and a
//! shared module was the last thing tying four planes to one compilation unit.
//! `vault`, `provider`, `memory` and `gate` each took their own the same way.

pub mod activity;
pub mod fleet;
pub mod lease;
pub mod renew;
pub mod report;
pub mod session;

pub use afd_state::sql::{
    ADMIN_STATE_ACTIVE, ADMIN_STATE_DRAINED, ADMIN_STATE_DRAINING, LAST_SEEN_NEVER,
    LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, LEASE_STATUS_REPORTED,
};

#[cfg(test)]
mod tests {
    //! The ledger invariant both charging statements have to hold together.
    //!
    //! Here rather than beside either statement, because the rule is about the
    //! pair: `report` and `renew` are the two writers of a charge row, and a
    //! rule only one of them obeys is not a rule. `report.rs` is also within
    //! fifty lines of the file-length cap, so a test module there would push
    //! the next edit over it.

    use super::{renew::RENEW_AND_METER, report::CLAIM_AND_SETTLE};

    /// The two charging statements, by the name a failure should print.
    const CHARGING_STATEMENTS: [(&str, &str); 2] = [
        ("report::CLAIM_AND_SETTLE", CLAIM_AND_SETTLE),
        ("renew::RENEW_AND_METER", RENEW_AND_METER),
    ];

    /// The clause that accumulates a re-sent charge into the existing row.
    const ACCUMULATE_CLAUSE: &str = "DO UPDATE SET";

    /// The column this module is careful about.
    const SNAPSHOT_COLUMN: &str = "fleet_name";

    /// Both writers capture the name, or a charge lands unattributable.
    ///
    /// A third charging statement added without this column would write rows
    /// that read "DELETED AGENT" the moment their fleet is purged — the exact
    /// defect slot 915 exists to end, reintroduced quietly. The set is asserted
    /// rather than each statement separately so a new member fails by default.
    #[test]
    fn test_m201_both_charging_statements_capture_the_fleet_name() {
        for (name, statement) in CHARGING_STATEMENTS {
            assert!(
                statement.contains(SNAPSHOT_COLUMN),
                "{name} writes a charge without capturing {SNAPSHOT_COLUMN}"
            );
            assert!(
                statement.contains("FROM core.fleets f WHERE f.id = g.fleet_id"),
                "{name} must read the name from the fleet its own row names, \
                 not from a parameter a caller could disagree with"
            );
        }
    }

    /// Neither accumulate path may re-stamp the name.
    ///
    /// This is the silent one. Every other column in the `DO UPDATE SET` list
    /// accumulates, so adding `fleet_name` beside them looks like symmetry and
    /// reads as a fix. It is not: renewals run every ~25 seconds of a live run,
    /// so a listed name would be re-stamped on every tick and a mid-run rename
    /// would rewrite the charge's whole history. A ledger records what was true
    /// when the money moved, and nothing here may edit that after the fact.
    #[test]
    fn test_m201_accumulate_paths_never_rewrite_the_captured_name() {
        for (name, statement) in CHARGING_STATEMENTS {
            // `assert!` then a defaulting split, rather than a panicking unwrap:
            // this module is compiled as `afd_fleet` lib-test, where the lane
            // denies `clippy::panic`, and the missing-clause case deserves its
            // own sentence anyway.
            assert!(
                statement.contains(ACCUMULATE_CLAUSE),
                "{name} has no {ACCUMULATE_CLAUSE} clause to check"
            );
            let accumulate = statement
                .split_once(ACCUMULATE_CLAUSE)
                .map_or("", |(_, tail)| tail);

            // Comments stripped: both statements carry prose explaining why the
            // column is absent here, and that prose names it.
            let assignments: String = accumulate
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");

            assert!(
                !assignments.contains(SNAPSHOT_COLUMN),
                "{name} re-stamps {SNAPSHOT_COLUMN} on every accumulate — a \
                 rename would rewrite history that already happened"
            );
        }
    }
}
