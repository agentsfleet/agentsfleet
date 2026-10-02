#![expect(
    clippy::unwrap_used,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;

use afd_core::id::Uuid7;

use super::StorageHome;
use crate::test_support::LEASE_ID;

#[test]
fn test_storage_home_sweeps_orphans_only() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let orphan = home.lease_dir(&Uuid7::parse(LEASE_ID).unwrap());
    fs::create_dir_all(orphan.join("workspace")).unwrap();
    let foreign_dir = root.path().join("leases/operator-notes");
    fs::create_dir_all(&foreign_dir).unwrap();
    let foreign_file = root
        .path()
        .join("leases/01890a5d-ac96-774b-bcce-b302099a8099");
    fs::write(&foreign_file, b"not a directory").unwrap();

    let swept = home.sweep().unwrap();

    assert_eq!(swept, 1);
    assert!(!orphan.exists(), "the orphaned lease directory is gone");
    assert!(
        foreign_dir.exists() && foreign_file.exists(),
        "nothing foreign is touched"
    );
    assert!(home.spool().is_dir() && home.bundles().is_dir());
}

#[test]
fn a_root_that_is_a_file_refuses_to_open() {
    let file = tempfile::NamedTempFile::new().unwrap();

    assert!(StorageHome::open(file.path()).is_err());
}
