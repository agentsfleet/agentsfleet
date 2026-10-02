#![expect(
    clippy::unwrap_used,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::io;
use std::path::Path;

use super::StorageHome;
use crate::test_support::LEASE_ID;

/// Another lease's directory, which a test makes unremovable.
const STUCK_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8099";

fn home() -> (tempfile::TempDir, StorageHome) {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    (root, home)
}

#[test]
fn test_storage_home_sweeps_orphans_only() {
    let (_root, home) = home();
    let orphan = home.sandboxes().join(LEASE_ID);
    fs::create_dir_all(orphan.join("workspace")).unwrap();
    let foreign_dir = home.sandboxes().join("slot-0");
    fs::create_dir_all(&foreign_dir).unwrap();
    let foreign_file = home.sandboxes().join(STUCK_LEASE);
    fs::write(&foreign_file, b"not a directory").unwrap();

    let swept = home.sweep();

    assert_eq!(swept, 1);
    assert!(!orphan.exists(), "the orphaned lease sandbox is gone");
    assert!(
        foreign_dir.exists() && foreign_file.exists(),
        "nothing foreign is touched"
    );
    assert!(home.spool().is_dir() && home.bundles().is_dir());
}

#[test]
fn a_directory_that_will_not_go_is_skipped_and_boot_goes_on() {
    let (_root, home) = home();
    for lease in [LEASE_ID, STUCK_LEASE] {
        fs::create_dir_all(home.sandboxes().join(lease)).unwrap();
    }
    let stuck = |orphan: &Path| {
        if orphan.ends_with(STUCK_LEASE) {
            Err(io::Error::other("busy: still mounted"))
        } else {
            fs::remove_dir_all(orphan)
        }
    };

    let swept = home.sweep_with(stuck);

    assert_eq!(swept, 1);
    assert!(home.sandboxes().join(STUCK_LEASE).exists());
    assert!(!home.sandboxes().join(LEASE_ID).exists());
}

#[test]
fn a_missing_sandbox_base_sweeps_nothing_rather_than_refusing() {
    let (_root, home) = home();
    fs::remove_dir(home.sandboxes()).unwrap();

    assert_eq!(home.sweep(), 0);
}

#[test]
fn a_root_that_is_a_file_refuses_to_open() {
    let file = tempfile::NamedTempFile::new().unwrap();

    assert!(StorageHome::open(file.path()).is_err());
}
