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

    for directory in [home.sandboxes(), home.spool(), home.bundles()] {
        assert!(directory.is_dir(), "{}", directory.display());
    }
    assert_eq!(home, again);
}

#[test]
fn a_root_that_is_a_file_refuses_to_open() {
    let file = tempfile::NamedTempFile::new().unwrap();

    assert!(StorageHome::open(file.path()).is_err());
}
