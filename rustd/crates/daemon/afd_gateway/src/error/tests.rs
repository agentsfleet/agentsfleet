//! Each failure keeps the code, retry answer and metric reason the runner and dashboards read.

use super::{Error, ErrorKind};
use crate::Operation;
use af_core::error_code::{self, ErrorCode};
use af_wire::protocol::StreamKind;

type Proof = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[track_caller]
fn assert_class(error: &Error, code: ErrorCode, detail: &str, retryable: bool, reason: &str) {
    assert_eq!(error.code(), code, "{error}");
    assert_eq!(error.detail(), detail, "{error}");
    assert_eq!(error.retryable(), retryable, "{error}");
    assert_eq!(error.reason(), reason, "{error}");
}

#[test]
fn refused_bindings_and_malformed_messages_never_ask_for_a_retry() -> Proof {
    let header = http::HeaderValue::from_bytes(b"\xff")?.to_str().map(drop);
    let identifier = af_core::id::Uuid7::parse("not-an-identifier").map(drop);
    let json = serde_json::from_str::<u8>("not json").map(drop);
    let cases = [
        (
            Error::from(ErrorKind::Binding),
            error_code::AUTH_FORBIDDEN,
            "Attach this kind to the physical connection that authenticated its session.",
            "binding",
        ),
        (
            Error::from(ErrorKind::DuplicateStream {
                kind: StreamKind::Work,
            }),
            error_code::INVALID_REQUEST,
            "The work kind is already attached. Open each kind once.",
            "binding",
        ),
        (
            Error::from(header.err().ok_or("a non-text header byte decoded")?),
            error_code::INVALID_REQUEST,
            "A runner stream header is not valid text.",
            "metadata",
        ),
        (
            Error::from(identifier.err().ok_or("a malformed identifier parsed")?),
            error_code::UUIDV7_INVALID_ID_SHAPE,
            "The message contains an invalid UUIDv7 identifier.",
            "protocol",
        ),
        (
            Error::from(json.err().ok_or("malformed JSON parsed")?),
            error_code::INVALID_REQUEST,
            "The message is not valid JSON for this runner protocol.",
            "protocol",
        ),
    ];
    for (error, code, detail, reason) in cases {
        assert_class(&error, code, detail, false, reason);
    }
    Ok(())
}

#[tokio::test]
async fn transient_failures_keep_the_lease_deadline_and_ask_for_a_retry() -> Proof {
    let elapsed = tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
        .await
        .err()
        .ok_or("a zero deadline did not elapse")?;
    let timeout = Error::from(ErrorKind::Timeout {
        operation: Operation::Renewal,
        source: elapsed,
    });
    assert_class(
        &timeout,
        error_code::INTERNAL_OPERATION_FAILED,
        "The stream timed out while renewing work. Retain the last confirmed lease deadline.",
        true,
        "timeout",
    );
    let io = Error::from(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
    assert_class(
        &io,
        error_code::INTERNAL_OPERATION_FAILED,
        "The connection ended. Reconnect before the last confirmed lease deadline.",
        true,
        "network",
    );
    let query = Error::from(sqlx::Error::PoolTimedOut);
    assert_class(
        &query,
        error_code::INTERNAL_DB_QUERY,
        "The database did not confirm the operation. Retain the last confirmed lease deadline.",
        true,
        "database",
    );
    let session_worker = tokio::spawn(std::future::pending::<()>());
    session_worker.abort();
    let worker = Error::from(
        session_worker
            .await
            .err()
            .ok_or("the aborted session worker joined")?,
    );
    assert_class(
        &worker,
        error_code::INTERNAL_OPERATION_FAILED,
        "A session worker stopped. Reconnect before the last confirmed lease deadline.",
        true,
        "worker",
    );
    let http = Error::from(
        http::Response::builder()
            .status(1_000_u16)
            .body(())
            .err()
            .ok_or("an out-of-range status built a response")?,
    );
    assert_class(
        &http,
        error_code::INTERNAL_OPERATION_FAILED,
        "The server could not construct its response. Retry with backoff.",
        true,
        "internal",
    );
    Ok(())
}

#[test]
fn every_operation_keeps_its_metric_label_and_sentence() {
    let table = [
        (
            Operation::Opening,
            "opening",
            "confirming the runner connection",
        ),
        (Operation::Offer, "offer", "offering work"),
        (Operation::Heartbeat, "heartbeat", "recording a heartbeat"),
        (Operation::Recheck, "recheck", "checking the current owner"),
        (Operation::Renewal, "renewal", "renewing work"),
        (Operation::Model, "model", "calling the authoring model"),
        (Operation::Activity, "activity", "publishing progress"),
        (Operation::Report, "report", "settling a report"),
        (Operation::Folds, "folds", "reading pending inputs"),
        (Operation::Write, "write", "sending a response"),
        (Operation::Drain, "drain", "closing the runner session"),
    ];
    for (operation, label, sentence) in table {
        assert_eq!(operation.as_str(), label);
        assert_eq!(operation.to_string(), sentence);
    }
}

#[test]
fn an_undelivered_frame_is_dropped_and_its_closed_channel_retries() -> Proof {
    let (sender, receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(1);
    drop(receiver);
    let undelivered = sender
        .try_send(b"private frame bytes".to_vec())
        .err()
        .map(|refused| match refused {
            tokio::sync::mpsc::error::TrySendError::Closed(bytes)
            | tokio::sync::mpsc::error::TrySendError::Full(bytes) => {
                tokio::sync::mpsc::error::SendError(bytes)
            }
        });
    let closed = super::closed_send(undelivered.ok_or("the closed channel accepted the frame")?);
    assert!(matches!(closed.kind(), ErrorKind::ChannelSend { .. }));
    assert!(closed.retryable());
    assert_eq!(closed.reason(), "closed");
    assert!(!format!("{closed:?}").contains("private"));
    Ok(())
}
