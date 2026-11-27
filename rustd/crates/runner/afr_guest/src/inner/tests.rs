#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use super::{clear, written};

/// A renewal empties the workspace and, beside it, the scratch space and the
/// shared memory a tenant also writes: each keeps its own directory, loses
/// every file, directory and link inside it, and follows no link out, so what
/// a link named survives.
#[test]
fn a_renewal_empties_every_tree_a_tenant_wrote_and_touches_nothing_a_link_named() {
    let outside = tempfile::tempdir().unwrap();
    let kept = outside.path().join("kept");
    fs::write(&kept, b"not the workspace's").unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let root = workspace.path();
    fs::write(root.join("file"), b"tenant").unwrap();
    fs::create_dir_all(root.join("tree/deeper")).unwrap();
    fs::write(root.join("tree/deeper/leaf"), b"tenant").unwrap();
    symlink(&kept, root.join("link")).unwrap();
    symlink(outside.path(), root.join("tree/out")).unwrap();
    fs::write(scratch.path().join("left"), b"tenant").unwrap();

    clear([root, scratch.path()]).unwrap();

    for tree in [root, scratch.path()] {
        assert!(tree.is_dir(), "the tree itself stays");
        assert_eq!(fs::read_dir(tree).unwrap().count(), 0, "{}", tree.display());
    }
    assert_eq!(fs::read(&kept).unwrap(), b"not the workspace's");
    let emptied: Vec<_> = written(Path::new("/workspace")).collect();
    assert_eq!(
        emptied,
        ["/workspace", "/tmp", "/dev/shm"].map(Path::new),
        "what a renewal empties"
    );
}
