#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::os::unix::fs::symlink;

use jsonrpsee_types::error::INVALID_PARAMS_CODE;

use super::tests::fixture;
use crate::protocol::{FILE_NOT_FOUND_CODE, PATH_REFUSED_CODE};

/// What two appends leave behind, the first of them making the file.
const FIRST: &[u8] = b"one\n";
const SECOND: &[u8] = b"two\n";

#[test]
fn an_append_makes_a_missing_file_and_adds_to_the_end_of_one_that_exists() {
    let (scratch, workspace) = fixture();

    workspace.append("log/run.txt", FIRST).unwrap();
    workspace.append("log/run.txt", SECOND).unwrap();

    let written = std::fs::read(scratch.path().join("workspace/log/run.txt")).unwrap();
    assert_eq!(written, [FIRST, SECOND].concat());
}

#[test]
fn a_delete_removes_a_regular_file_and_nothing_else() {
    let (scratch, workspace) = fixture();
    let root = scratch.path().join("workspace");
    std::fs::write(root.join("gone"), b"x").unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    symlink(scratch.path().join("outside/secret"), root.join("alias")).unwrap();

    workspace.delete("gone").unwrap();
    let directory = workspace.delete("dir").unwrap_err();
    let link = workspace.delete("alias").unwrap_err();
    let missing = workspace.delete("gone").unwrap_err();

    assert!(!root.join("gone").exists());
    for refused in [&directory, &link] {
        assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
        assert!(refused.wire_message().contains("not a regular file"));
    }
    assert!(root.join("dir").is_dir(), "the directory stands");
    assert!(
        root.join("alias").symlink_metadata().unwrap().is_symlink(),
        "the link stands"
    );
    assert!(
        scratch.path().join("outside/secret").exists(),
        "and so does what it points at"
    );
    assert_eq!(missing.rpc_code(), FILE_NOT_FOUND_CODE, "{missing}");
}

#[test]
fn an_append_and_a_delete_refuse_a_path_out_like_every_other_call() {
    let (scratch, workspace) = fixture();
    symlink(
        scratch.path().join("outside"),
        scratch.path().join("workspace/link"),
    )
    .unwrap();

    let appended = workspace.append("link/planted", b"x").unwrap_err();
    let deleted = workspace.delete("../outside/secret").unwrap_err();
    let through_link = workspace.delete("link/secret").unwrap_err();

    for refused in [appended, deleted, through_link] {
        assert_eq!(refused.rpc_code(), PATH_REFUSED_CODE, "{refused}");
        assert!(refused.is_path_refused() && !refused.is_not_found());
    }
    assert!(!scratch.path().join("outside/planted").exists());
    assert!(scratch.path().join("outside/secret").exists());
}

/// An append opens like a write: a directory is the caller's mistake, and a
/// pipe with no reader is refused as not a regular file, without waiting.
#[test]
fn an_append_refuses_a_directory_and_a_pipe_like_a_write() {
    let (scratch, workspace) = fixture();
    let root = scratch.path().join("workspace");
    std::fs::create_dir(root.join("dir")).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(root.join("fifo"))
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo made the pipe");

    let directory = workspace.append("dir", b"x").unwrap_err();
    let pipe = workspace.append("fifo", b"x").unwrap_err();

    assert_eq!(directory.rpc_code(), INVALID_PARAMS_CODE, "{directory}");
    assert_eq!(pipe.rpc_code(), INVALID_PARAMS_CODE, "{pipe}");
    assert!(pipe.wire_message().contains("not a regular file"), "{pipe}");
    assert!(root.join("dir").is_dir(), "the directory stands");
}
