//! The fake executor's file calls answer as its write mode says.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afr_executor::Executor as _;
use bytes::Bytes;
use tokio::sync::mpsc;

use super::{FakeExecutor, Writes};

#[tokio::test]
async fn an_append_is_reported_as_a_write_and_a_delete_answers_the_mode() {
    let (written, mut reported) = mpsc::unbounded_channel();
    let accepting = FakeExecutor {
        written: Some(written),
        writes: Writes::Accept,
        ..FakeExecutor::default()
    };
    let refusing = FakeExecutor {
        written: None,
        writes: Writes::Refuse,
        ..FakeExecutor::default()
    };

    accepting
        .append_file("notes.md", Bytes::from_static(b"more"))
        .await
        .unwrap();
    accepting.delete_file("notes.md").await.unwrap();
    let refused = refusing.delete_file("notes.md").await.unwrap_err();

    assert_eq!(
        reported.try_recv().unwrap(),
        ("notes.md".to_owned(), Bytes::from_static(b"more"))
    );
    assert!(refused.wire_message().contains("read-only"), "{refused}");
}
