//! File calls through the client.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afr_executor::{EntryKind, Executor as _};
use bytes::Bytes;

use crate::support::{MIB, start};

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
    assert_eq!(listed.len(), 1);
    let entry = listed.first().unwrap();
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
        assert!(refused.is_path_refused(), "{refused}");
        assert!(!refused.is_unknown_process());
    }
}

#[tokio::test]
async fn a_missing_file_is_a_failure_the_caller_can_tell_from_a_refusal() {
    let harness = start().await;

    let missing = harness.client.read_file("absent", 8).await.unwrap_err();

    assert!(!missing.is_path_refused());
    assert!(
        std::error::Error::source(&missing).is_none(),
        "the executor's reason travels as text, not as a cause"
    );
    assert!(missing.to_string().contains("No such file"), "{missing}");
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

    assert!(!refused.is_connection_lost(), "{refused}");
    assert!(
        still_open.is_empty(),
        "the connection survived and nothing was written"
    );
}

#[tokio::test]
async fn a_listing_through_the_client_names_every_kind() {
    let harness = start().await;
    std::fs::create_dir(harness.root.join("dir")).unwrap();
    std::os::unix::fs::symlink("dir", harness.root.join("link")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(harness.root.join("sock")).unwrap();

    let mut listed = harness.client.list_dir(".").await.unwrap();
    listed.sort_by(|left, right| left.name.cmp(&right.name));

    let kinds: Vec<EntryKind> = listed.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        [EntryKind::Directory, EntryKind::Symlink, EntryKind::Other]
    );
}
