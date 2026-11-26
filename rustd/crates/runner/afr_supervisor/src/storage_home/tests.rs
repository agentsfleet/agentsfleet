#![expect(
    clippy::unwrap_used,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use super::StorageHome;

#[test]
fn opening_a_home_makes_its_directories_and_opening_it_again_keeps_them() {
    let root = tempfile::tempdir().unwrap();

    let home = StorageHome::open(root.path()).unwrap();
    let again = StorageHome::open(root.path()).unwrap();

    for directory in [home.sandboxes(), home.mirrors(), home.toolbox()] {
        assert!(directory.is_dir(), "{}", directory.display());
    }
    assert_eq!(home, again);
}

#[test]
fn a_root_that_is_a_file_refuses_to_open() {
    let file = tempfile::NamedTempFile::new().unwrap();

    assert!(StorageHome::open(file.path()).is_err());
}

/// The toolbox sits in one `toolbox/` under the home, where the engine that
/// verifies it lays out its release, images and mounts.
#[test]
fn the_toolbox_sits_under_the_home() {
    let root = tempfile::tempdir().unwrap();

    let home = StorageHome::open(root.path()).unwrap();

    assert_eq!(home.toolbox(), root.path().join("toolbox"));
}

/// Completed reports never create a runner-side disk outbox.
#[test]
fn opening_storage_does_not_create_report_storage() {
    let root = tempfile::tempdir().unwrap();
    StorageHome::open(root.path()).unwrap();
    assert!(!root.path().join("spool").exists());
    assert!(!root.path().join("reports").exists());
    assert!(!root.path().join("receipts").exists());
}
