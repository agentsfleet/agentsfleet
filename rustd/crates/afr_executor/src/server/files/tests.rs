#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::os::unix::fs::symlink;

use base64::prelude::{BASE64_STANDARD, Engine as _};

use super::Workspace;
use crate::protocol::KindWire;

/// A workspace beside a directory outside it, with a secret in the outside one.
fn fixture() -> (tempfile::TempDir, Workspace) {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(scratch.path().join("outside")).unwrap();
    std::fs::write(scratch.path().join("outside/secret"), b"secret").unwrap();
    let workspace = Workspace::open(&root).unwrap();
    (scratch, workspace)
}

#[test]
fn test_executor_refuses_path_escape() {
    let (scratch, workspace) = fixture();
    symlink(
        scratch.path().join("outside"),
        scratch.path().join("workspace/link"),
    )
    .unwrap();
    let outside = scratch.path().join("outside/secret");

    for refused in [
        workspace.read("../outside/secret", 64).unwrap_err(),
        workspace.read("link/secret", 64).unwrap_err(),
        workspace.read(outside.to_str().unwrap(), 64).unwrap_err(),
        workspace.write("link/planted", b"x").unwrap_err(),
        workspace.list("link").unwrap_err(),
        workspace.directory(Some("../outside")).unwrap_err(),
    ] {
        assert!(refused.is_path_refused(), "{refused}");
    }
    assert!(
        !scratch.path().join("outside/planted").exists(),
        "nothing was written outside"
    );
}

#[test]
fn an_absolute_path_under_the_root_is_the_same_file() {
    let (scratch, workspace) = fixture();
    workspace.write("note", b"kept").unwrap();
    let absolute = scratch.path().join("workspace/note");

    let read = workspace.read(absolute.to_str().unwrap(), 64).unwrap();

    assert_eq!(BASE64_STANDARD.decode(read.content).unwrap(), b"kept");
    assert!(!read.truncated);
}

#[test]
fn a_read_past_its_limit_is_truncated_and_says_so() {
    let (_scratch, workspace) = fixture();
    workspace.write("long", b"0123456789").unwrap();

    let read = workspace.read("long", 4).unwrap();

    assert_eq!(BASE64_STANDARD.decode(read.content).unwrap(), b"0123");
    assert!(read.truncated);
}

#[test]
fn a_listing_names_each_kind_and_size() {
    let (scratch, workspace) = fixture();
    let root = scratch.path().join("workspace");
    std::fs::write(root.join("file"), b"abc").unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    symlink("file", root.join("alias")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(root.join("sock")).unwrap();

    let mut listed = workspace.list("").unwrap().entries;
    listed.sort_by(|left, right| left.name.cmp(&right.name));

    let kinds: Vec<(String, KindWire, u64)> = listed
        .into_iter()
        .map(|entry| (entry.name, entry.kind, entry.size))
        .collect();
    assert!(matches!(kinds.as_slice(), [
        (alias, KindWire::Symlink, _),
        (dir, KindWire::Directory, _),
        (file, KindWire::File, 3),
        (sock, KindWire::Other, _),
    ] if alias == "alias" && dir == "dir" && file == "file" && sock == "sock"));
}

#[test]
fn the_working_directory_is_the_root_unless_one_is_named() {
    let (scratch, workspace) = fixture();
    std::fs::create_dir(scratch.path().join("workspace/sub")).unwrap();

    assert_eq!(
        workspace.directory(None).unwrap(),
        scratch.path().join("workspace/.")
    );
    assert_eq!(
        workspace.directory(Some("sub")).unwrap(),
        scratch.path().join("workspace/sub")
    );
}

#[test]
fn a_missing_file_is_a_failure_but_not_a_refused_path() {
    let (_scratch, workspace) = fixture();

    let missing = workspace.read("absent", 8).unwrap_err();

    assert!(!missing.is_path_refused());
    assert!(
        missing.wire_message().contains(": "),
        "the cause rides along: {}",
        missing.wire_message()
    );
    assert!(
        !workspace
            .directory(Some("absent"))
            .unwrap_err()
            .is_path_refused()
    );
}
