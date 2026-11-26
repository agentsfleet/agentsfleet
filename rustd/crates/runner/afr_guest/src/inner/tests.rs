#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::os::unix::fs::symlink;

use super::clear;

/// The workspace a renewal empties keeps its own directory, loses every file,
/// directory and link inside it, and follows no link out: what a link named
/// survives.
#[test]
fn a_cleared_workspace_is_empty_and_nothing_a_link_named_is_touched() {
    let outside = tempfile::tempdir().unwrap();
    let kept = outside.path().join("kept");
    fs::write(&kept, b"not the workspace's").unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    fs::write(root.join("file"), b"tenant").unwrap();
    fs::create_dir_all(root.join("tree/deeper")).unwrap();
    fs::write(root.join("tree/deeper/leaf"), b"tenant").unwrap();
    symlink(&kept, root.join("link")).unwrap();
    symlink(outside.path(), root.join("tree/out")).unwrap();

    clear(root).unwrap();

    assert!(root.is_dir(), "the workspace itself stays");
    assert_eq!(
        fs::read_dir(root).unwrap().count(),
        0,
        "nothing is left in it"
    );
    assert_eq!(fs::read(&kept).unwrap(), b"not the workspace's");
}
