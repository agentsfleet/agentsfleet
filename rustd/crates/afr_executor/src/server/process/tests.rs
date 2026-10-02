#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afd_core::test_util::trace::Capture;
use bytes::Bytes;
use jsonrpsee_types::error::CALL_EXECUTION_FAILED_CODE;
use tokio::sync::mpsc;
use tracing::Level;

use super::{EVENT_PROCESS_COMPLETED, EVENT_PROCESS_FAILED, queue, report};
use crate::api::Ending;

#[test]
fn an_ending_with_no_status_is_logged_failed_and_any_other_completed() {
    let capture = Capture::install();

    report(3, Ending::Interrupted, 0);
    report(4, Ending::Exited(0), 9);

    let failed = capture.only(EVENT_PROCESS_FAILED);
    let completed = capture.only(EVENT_PROCESS_COMPLETED);
    assert_eq!(failed.level, Level::WARN);
    assert_eq!(failed.field("error_code"), Some("UZ-INTERNAL-003"));
    assert_eq!(completed.level, Level::DEBUG);
    assert_eq!(completed.field("omitted_bytes"), Some("9"));
}

#[test]
fn a_write_past_the_queue_or_after_the_input_closed_is_refused() {
    let (writes, queued) = mpsc::channel(1);

    queue(&writes, Bytes::from_static(b"a")).unwrap();
    let full = queue(&writes, Bytes::from_static(b"b")).unwrap_err();
    drop(queued);
    let closed = queue(&writes, Bytes::from_static(b"c")).unwrap_err();

    assert_eq!(full.rpc_code(), CALL_EXECUTION_FAILED_CODE);
    assert!(
        closed.wire_message().contains("roken pipe"),
        "{}",
        closed.wire_message()
    );
}
