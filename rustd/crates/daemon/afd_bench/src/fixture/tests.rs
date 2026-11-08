//! What the prefix promises the sweep, and what the ledger promises the result
//! file.

use super::{FixtureLedger, PREFIX_TOKEN, RunPrefix};

#[test]
fn test_a_prefix_disowns_another_runs_names() {
    let ours = RunPrefix::mint();

    assert!(
        !ours.owns("bench-1-2-fleet-7"),
        "sweeping by prefix must not reach another run's objects"
    );
}

#[test]
fn test_a_minted_prefix_owns_the_names_it_builds() {
    let prefix = RunPrefix::mint();

    let name = prefix.name("fleet-7");

    assert!(
        prefix.owns(&name),
        "a name the prefix built must be one the sweep recognises"
    );
    assert!(
        name.starts_with(PREFIX_TOKEN),
        "every created name leads with the bench token, so a human reading a \
         console can tell what made it"
    );
}

#[test]
fn test_a_fresh_ledger_is_balanced_because_it_created_nothing() {
    let ledger = FixtureLedger::new();

    assert_eq!(ledger.created_count(), 0);
    assert_eq!(ledger.swept_count(), 0);
    assert!(
        ledger.is_balanced(),
        "a run that created nothing has nothing outstanding"
    );
}

#[test]
fn test_a_ledger_is_unbalanced_until_the_sweep_accounts_for_everything() {
    let mut ledger = FixtureLedger::new();

    ledger.created(10);
    assert!(
        !ledger.is_balanced(),
        "objects created and not yet swept must read as outstanding"
    );

    ledger.swept(9);
    assert!(
        !ledger.is_balanced(),
        "a sweep that missed one must not report success"
    );

    ledger.swept(1);
    assert!(ledger.is_balanced());
    assert_eq!(ledger.created_count(), 10);
    assert_eq!(ledger.swept_count(), 10);
}

#[test]
fn test_sweeping_an_earlier_runs_orphans_keeps_the_ledger_balanced() {
    let mut ledger = FixtureLedger::new();

    ledger.created(4);
    ledger.swept(6);

    assert!(
        ledger.is_balanced(),
        "a sweep removing this run's objects plus an earlier run's orphans is \
         a clean outcome, not an accounting error"
    );
}

#[test]
fn test_the_ledger_saturates_rather_than_wrapping() {
    let mut ledger = FixtureLedger::new();

    ledger.created(u64::MAX);
    ledger.created(1);

    assert_eq!(
        ledger.created_count(),
        u64::MAX,
        "a count that wrapped to zero would report a clean sweep of everything"
    );
}
