#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::os::unix::fs::symlink;

use jsonrpsee_types::error::INVALID_PARAMS_CODE;

use super::tests::fixture;
use af_guest_wire::protocol::{FILE_NOT_FOUND_CODE, PATH_REFUSED_CODE};

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
fn a_delete_refuses_a_path_out_like_every_other_call() {
    let (scratch, workspace) = fixture();
    symlink(
        scratch.path().join("outside"),
        scratch.path().join("workspace/link"),
    )
    .unwrap();

    let deleted = workspace.delete("../outside/secret").unwrap_err();
    let through_link = workspace.delete("link/secret").unwrap_err();

    for refused in [deleted, through_link] {
        assert_eq!(refused.rpc_code(), PATH_REFUSED_CODE, "{refused}");
        assert!(refused.is_path_refused() && !refused.is_not_found());
    }
    assert!(scratch.path().join("outside/secret").exists());
}
