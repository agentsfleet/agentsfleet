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

    for directory in [
        home.sandboxes(),
        home.spool(),
        home.bundles(),
        home.mirrors(),
        home.toolbox_incoming(),
        home.toolbox_images(),
        home.toolbox_mounts(),
    ] {
        assert!(directory.is_dir(), "{}", directory.display());
    }
    assert_eq!(home, again);
}

#[test]
fn a_root_that_is_a_file_refuses_to_open() {
    let file = tempfile::NamedTempFile::new().unwrap();

    assert!(StorageHome::open(file.path()).is_err());
}

/// The toolbox's three directories sit apart under one `toolbox/`: the deploy
/// stages into `incoming`, admission publishes into `images`, and nothing a
/// deploy stages can land where an admitted image is published or mounted.
#[test]
fn the_toolbox_directories_are_three_siblings_under_the_home() {
    let root = tempfile::tempdir().unwrap();

    let home = StorageHome::open(root.path()).unwrap();

    let toolbox = root.path().join("toolbox");
    assert_eq!(home.toolbox_incoming(), toolbox.join("incoming"));
    assert_eq!(home.toolbox_images(), toolbox.join("images"));
    assert_eq!(home.toolbox_mounts(), toolbox.join("mounts"));
}
