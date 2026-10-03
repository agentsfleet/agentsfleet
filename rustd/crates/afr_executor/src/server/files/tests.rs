#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::os::unix::fs::{OpenOptionsExt as _, symlink};

use jsonrpsee_types::error::INVALID_PARAMS_CODE;

use super::{MAX_LIST_ENTRIES, Workspace};
use crate::api::EntryKind;
use crate::protocol::PATH_REFUSED_CODE;

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
        assert_eq!(refused.rpc_code(), PATH_REFUSED_CODE, "{refused}");
    }
    assert!(
        !scratch.path().join("outside/planted").exists(),
        "nothing was written outside"
    );
}

/// A bundle's support files land in nested directories the workspace does
/// not have yet.
#[test]
fn a_write_makes_its_missing_parent_directories() {
    let (scratch, workspace) = fixture();

    workspace.write("docs/guides/setup.md", b"guide").unwrap();

    let written = std::fs::read(scratch.path().join("workspace/docs/guides/setup.md")).unwrap();
    assert_eq!(written, b"guide");
}

/// Making parents never follows a link out of the workspace.
#[test]
fn a_write_never_makes_parents_through_a_link_out() {
    let (scratch, workspace) = fixture();
    symlink(
        scratch.path().join("outside"),
        scratch.path().join("workspace/link"),
    )
    .unwrap();

    let refused = workspace.write("link/deeper/planted", b"x").unwrap_err();

    assert_eq!(refused.rpc_code(), PATH_REFUSED_CODE, "{refused}");
    assert!(!scratch.path().join("outside/deeper").exists());
}

#[test]
fn an_absolute_path_under_the_root_is_the_same_file() {
    let (scratch, workspace) = fixture();
    workspace.write("note", b"kept").unwrap();
    let absolute = scratch.path().join("workspace/note");

    let read = workspace.read(absolute.to_str().unwrap(), 64).unwrap();

    assert_eq!(read.content, b"kept".as_slice());
    assert!(!read.truncated);
}

#[test]
fn a_read_past_its_limit_is_truncated_and_says_so() {
    let (_scratch, workspace) = fixture();
    workspace.write("long", b"0123456789").unwrap();

    let read = workspace.read("long", 4).unwrap();

    assert_eq!(read.content, b"0123".as_slice());
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

    let kinds: Vec<(String, EntryKind, u64)> = listed
        .into_iter()
        .map(|entry| (entry.name, entry.kind, entry.size))
        .collect();
    assert!(matches!(kinds.as_slice(), [
        (alias, EntryKind::Symlink, _),
        (dir, EntryKind::Directory, _),
        (file, EntryKind::File, 3),
        (sock, EntryKind::Other, _),
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
fn a_missing_name_is_the_callers_mistake_not_a_refused_path() {
    let (_scratch, workspace) = fixture();

    let missing = workspace.read("absent", 8).unwrap_err();
    let no_directory = workspace.directory(Some("absent")).unwrap_err();

    for refused in [&missing, &no_directory] {
        assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
    }
    assert!(
        missing.wire_message().contains(": "),
        "the cause rides along: {}",
        missing.wire_message()
    );
}

#[test]
fn a_real_permission_refusal_is_told_apart_from_an_escape() {
    let (scratch, workspace) = fixture();
    let locked = scratch.path().join("workspace/locked");
    std::fs::write(&locked, b"x").unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
    if rustix::process::geteuid().is_root() {
        // Root reads through mode bits, so there is no refusal to sort.
        return;
    }

    let refused = workspace.read("locked", 8).unwrap_err();

    assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
    assert!(
        refused.wire_message().contains("ermission denied"),
        "{}",
        refused.wire_message()
    );
}

#[test]
fn a_pipe_is_refused_without_waiting_for_a_writer_or_a_reader() {
    let (scratch, workspace) = fixture();
    let fifo = scratch.path().join("workspace/fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo made the pipe");

    let read = workspace.read("fifo", 8).unwrap_err();
    let written = workspace.write("fifo", b"x").unwrap_err();
    // With a reader waiting, a non-blocking open for writing succeeds, and
    // the type check after it is what refuses.
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed())
        .open(&fifo)
        .unwrap();
    let written_to_reader = workspace.write("fifo", b"x").unwrap_err();
    drop(reader);

    for refused in [read, written, written_to_reader] {
        assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
        assert!(refused.wire_message().contains("not a regular file"));
    }
}

#[test]
fn a_directory_past_the_cap_is_listed_up_to_it_and_says_so() {
    let (scratch, workspace) = fixture();
    let crowded = scratch.path().join("workspace/crowded");
    std::fs::create_dir(&crowded).unwrap();
    for name in 0..=MAX_LIST_ENTRIES {
        std::fs::write(crowded.join(name.to_string()), b"").unwrap();
    }
    std::fs::remove_file(crowded.join("0")).unwrap();

    let exactly = workspace.list("crowded").unwrap();
    std::fs::write(crowded.join("0"), b"").unwrap();
    let past = workspace.list("crowded").unwrap();

    assert_eq!(exactly.entries.len(), MAX_LIST_ENTRIES);
    assert!(!exactly.truncated, "a full listing that fits is not cut");
    assert_eq!(past.entries.len(), MAX_LIST_ENTRIES);
    assert!(past.truncated);
}

#[test]
fn a_write_reports_permission_denied_when_it_cannot_make_parents() {
    if rustix::process::geteuid().is_root() {
        // Root bypasses the mode-bit refusal this test exercises.
        return;
    }
    let (scratch, workspace) = fixture();
    let locked = scratch.path().join("workspace/ro");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    let refused = workspace.write("ro/sub/file", b"x").unwrap_err();
    // Restore permissions before TempDir removes the fixture, even if an assertion fails.
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
    assert!(
        refused.wire_message().contains("ermission denied"),
        "{}",
        refused.wire_message()
    );
    assert!(
        !locked.join("sub").exists(),
        "no partial directory was made"
    );
}

#[test]
fn a_write_reports_a_parent_name_the_filesystem_cannot_create() {
    // Beyond the component limit, with a missing directory before it so the
    // initial open reports NotFound and parent creation encounters the refusal.
    const OVERLONG_COMPONENT_BYTES: usize = 256;
    let (scratch, workspace) = fixture();
    let path = format!("missing/{}/file", "x".repeat(OVERLONG_COMPONENT_BYTES));
    let refused = workspace.write(&path, b"x").unwrap_err();
    assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE, "{refused}");
    assert!(refused.wire_message().contains("too long"), "{refused}");
    assert!(scratch.path().join("workspace/missing").is_dir());
    assert_eq!(
        std::fs::read_dir(scratch.path().join("workspace/missing"))
            .unwrap()
            .count(),
        0,
        "no file or overlong directory was created"
    );
}
