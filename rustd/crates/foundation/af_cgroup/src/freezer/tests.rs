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

/// A freeze the kernel refuses to take is never waited out: the refusal
/// names `cgroup.freeze`, the file that refused it.
#[test]
fn test_a_freeze_the_kernel_refuses_names_its_control_file() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = reporting(dir.path(), SETTLED_FROZEN);
    // A directory where the control file goes refuses every write.
    fs::create_dir(dir.path().join("cgroup.freeze")).unwrap();

    let refused = freezer.freeze().unwrap_err().to_string();

    assert!(refused.contains("cgroup.freeze"), "{refused}");
}

/// The refusal says the file could not be read: it was never written.
#[test]
fn test_an_unreadable_events_file_ends_the_wait_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = Freezer::new(dir.path());

    let refused = freezer.freeze().unwrap_err().to_string();

    assert!(
        refused.contains("the cgroup file cgroup.events could not be read"),
        "{refused}"
    );
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

/// A thaw the kernel never reports is refused, naming the state it never
/// reached.
#[test]
fn test_a_thaw_the_kernel_never_reports_is_refused_as_unsettled() {
    let dir = tempfile::tempdir().unwrap();
    let freezer = reporting(dir.path(), SETTLED_FROZEN);

    let refused = freezer.thaw().unwrap_err().to_string();

    assert!(refused.contains("did not settle thawed"), "{refused}");
}

/// A freezer opened up front keeps its two files: the cgroup's directory
/// moved away, it still writes the request and reads the tree settled.
#[test]
fn test_an_opened_freezer_freezes_and_thaws_through_the_files_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let cgroup = dir.path().join("tenant");
    fs::create_dir(&cgroup).unwrap();
    fs::write(cgroup.join("cgroup.freeze"), "0").unwrap();
    fs::write(cgroup.join("cgroup.events"), SETTLED_FROZEN).unwrap();
    let freezer = Freezer::opened(&cgroup).unwrap();
    let moved = dir.path().join("moved");
    fs::rename(&cgroup, &moved).unwrap();

    freezer.freeze().unwrap();
    let frozen = fs::read_to_string(moved.join("cgroup.freeze")).unwrap();
    fs::write(moved.join("cgroup.events"), SETTLED_THAWED).unwrap();
    freezer.thaw().unwrap();

    assert_eq!(frozen, "1");
    assert_eq!(
        fs::read_to_string(moved.join("cgroup.freeze")).unwrap(),
        "0"
    );
    assert!(!cgroup.exists());
}

/// A cgroup with no freezer file refuses the open, naming the file.
#[test]
fn test_a_freezer_is_not_opened_over_a_cgroup_without_its_files() {
    let dir = tempfile::tempdir().unwrap();

    let refused = Freezer::opened(dir.path()).unwrap_err().to_string();

    assert!(refused.contains("cgroup.freeze"), "{refused}");
}
