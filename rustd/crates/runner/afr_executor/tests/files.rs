//! File calls through the client.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afr_executor::{EntryKind, Executor as _};
use bytes::Bytes;

use crate::support::{
    FILE_NOT_FOUND, INVALID_PARAMS, MIB, PATH_REFUSED, PATIENCE, UNKNOWN_PROCESS, is_lost,
    refused_with, start,
};

#[tokio::test]
async fn a_written_file_reads_back_and_lists() {
    let harness = start().await;

    harness
        .client
        .write_file("notes.txt", Bytes::from_static(b"twelve bytes"))
        .await
        .unwrap();
    let whole = harness.client.read_file("notes.txt", 64).await.unwrap();
    let cut = harness.client.read_file("notes.txt", 6).await.unwrap();
    let listed = harness.client.list_dir(".").await.unwrap();

    assert_eq!(whole.data.as_ref(), b"twelve bytes");
    assert!(!whole.truncated);
    assert_eq!(cut.data.as_ref(), b"twelve");
    assert!(cut.truncated);
    assert!(!listed.truncated);
    assert_eq!(listed.entries.len(), 1);
    let entry = listed.entries.first().unwrap();
    assert_eq!(
        (entry.name.as_str(), entry.kind, entry.size),
        ("notes.txt", EntryKind::File, 12)
    );
}

#[tokio::test]
async fn a_path_outside_the_workspace_is_refused_by_name() {
    let harness = start().await;

    let read = harness.client.read_file("../escape", 8).await.unwrap_err();
    let written = harness
        .client
        .write_file("/etc/planted", Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    let listed = harness.client.list_dir("..").await.unwrap_err();

    for refused in [read, written, listed] {
        assert!(refused_with(&refused, PATH_REFUSED), "{refused}");
        assert!(!refused_with(&refused, UNKNOWN_PROCESS));
        assert!(refused.is_path_refused(), "{refused}");
    }
}

#[tokio::test]
async fn a_missing_file_is_a_failure_the_caller_can_tell_from_a_refusal() {
    let harness = start().await;

    let missing = harness.client.read_file("absent", 8).await.unwrap_err();

    assert!(
        refused_with(&missing, FILE_NOT_FOUND),
        "a missing name is the caller's to fix: {missing}"
    );
    assert!(missing.is_not_found(), "{missing}");
    assert!(
        std::error::Error::source(&missing).is_none(),
        "the executor's reason travels as text, not as a cause"
    );
    assert!(missing.to_string().contains("No such file"), "{missing}");
}

#[tokio::test]
async fn an_append_grows_a_file_and_a_delete_removes_it() {
    let harness = start().await;

    for line in [b"one\n", b"two\n"] {
        harness
            .client
            .append_file("log", Bytes::from_static(line))
            .await
            .unwrap();
    }
    let grown = harness.client.read_file("log", 64).await.unwrap();
    harness.client.delete_file("log").await.unwrap();
    let gone = harness.client.read_file("log", 64).await.unwrap_err();
    let again = harness.client.delete_file("log").await.unwrap_err();

    assert_eq!(grown.data.as_ref(), b"one\ntwo\n");
    assert!(gone.is_not_found(), "{gone}");
    assert!(refused_with(&again, FILE_NOT_FOUND), "{again}");
}

#[tokio::test]
async fn a_write_too_long_for_one_message_is_refused_before_it_is_sent() {
    let harness = start().await;

    let refused = harness
        .client
        .write_file("huge", Bytes::from(vec![0_u8; 16 * MIB]))
        .await
        .unwrap_err();
    let still_open = harness.client.list_dir(".").await.unwrap();

    assert!(!is_lost(&refused), "{refused}");
    assert!(
        still_open.entries.is_empty(),
        "the connection survived and nothing was written"
    );
}

#[tokio::test]
async fn a_listing_through_the_client_names_every_kind() {
    let harness = start().await;
    std::fs::create_dir(harness.root.join("dir")).unwrap();
    std::os::unix::fs::symlink("dir", harness.root.join("link")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(harness.root.join("sock")).unwrap();

    let mut listed = harness.client.list_dir(".").await.unwrap().entries;
    listed.sort_by(|left, right| left.name.cmp(&right.name));

    let kinds: Vec<EntryKind> = listed.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        [EntryKind::Directory, EntryKind::Symlink, EntryKind::Other]
    );
}

#[tokio::test]
async fn a_pipe_is_refused_for_reading_and_writing_without_blocking() {
    let harness = start().await;
    let fifo = harness.root.join("fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo made the pipe");

    let read = tokio::time::timeout(PATIENCE, harness.client.read_file("fifo", 8)).await;
    let written = tokio::time::timeout(
        PATIENCE,
        harness.client.write_file("fifo", Bytes::from_static(b"x")),
    )
    .await;

    for refused in [read.unwrap().unwrap_err(), written.unwrap().unwrap_err()] {
        assert!(refused_with(&refused, INVALID_PARAMS), "{refused}");
        assert!(
            refused.to_string().contains("not a regular file"),
            "{refused}"
        );
    }
}

#[tokio::test]
async fn a_directory_given_as_a_file_is_the_callers_mistake() {
    let harness = start().await;
    std::fs::create_dir(harness.root.join("dir")).unwrap();

    let read = harness.client.read_file("dir", 8).await.unwrap_err();
    let written = harness
        .client
        .write_file("dir", Bytes::from_static(b"x"))
        .await
        .unwrap_err();
    let listed = harness.client.list_dir("absent").await.unwrap_err();

    for refused in [read, written] {
        assert!(refused_with(&refused, INVALID_PARAMS), "{refused}");
    }
    assert!(refused_with(&listed, FILE_NOT_FOUND), "{listed}");
}

#[tokio::test]
async fn a_listing_past_its_cap_says_it_was_cut() {
    let harness = start().await;
    let crowded = harness.root.join("crowded");
    std::fs::create_dir(&crowded).unwrap();
    for name in 0..=4_096 {
        std::fs::write(crowded.join(name.to_string()), b"").unwrap();
    }

    let listed = harness.client.list_dir("crowded").await.unwrap();

    assert_eq!(listed.entries.len(), 4_096);
    assert!(listed.truncated);
}
