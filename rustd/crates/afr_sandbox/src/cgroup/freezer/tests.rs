#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::fs;
use std::path::Path;

use super::Freezer;

/// What a kernel publishes for a cgroup whose tree is all stopped.
const SETTLED_FROZEN: &str = "populated 1\nfrozen 1\n";
/// What it publishes for one whose tree runs.
const SETTLED_THAWED: &str = "populated 1\nfrozen 0\n";

/// A freezer over a plain directory whose `cgroup.events` reads `events`.
fn reporting(dir: &Path, events: &str) -> Freezer {
    fs::write(dir.join("cgroup.events"), events).unwrap();
    Freezer::new(dir)
}

#[test]
fn test_freeze_writes_the_request_and_returns_once_the_tree_reports_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = reporting(dir.path(), SETTLED_FROZEN);

    freezer.freeze().unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("cgroup.freeze")).unwrap(),
        "1"
    );
}

#[test]
fn test_thaw_writes_the_request_and_returns_once_the_tree_reports_running() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = reporting(dir.path(), SETTLED_THAWED);

    freezer.thaw().unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("cgroup.freeze")).unwrap(),
        "0"
    );
}

#[test]
fn test_a_freeze_the_kernel_never_reports_is_refused_as_unsettled() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = reporting(dir.path(), SETTLED_THAWED);

    let refused = freezer.freeze().unwrap_err().to_string();

    assert!(refused.contains("did not settle frozen"), "{refused}");
}

#[test]
fn test_an_unreadable_events_file_ends_the_wait_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = Freezer::new(dir.path());

    let refused = freezer.freeze().unwrap_err().to_string();

    assert!(refused.contains("cgroup.events"), "{refused}");
}

#[test]
fn test_frozen_is_read_from_its_own_key_and_value_only() {
    let dir = tempfile::tempdir().unwrap();
    for (events, frozen) in [
        (SETTLED_FROZEN, true),
        (SETTLED_THAWED, false),
        ("populated 1\n", false),
        ("frozen 10\n", false),
        ("unfrozen 1\nfrozen 0\n", false),
    ] {
        let freezer = reporting(dir.path(), events);
        assert_eq!(freezer.is_frozen().unwrap(), frozen, "{events:?}");
    }
}
