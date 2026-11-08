//! A connected executor can acknowledge spawn and then withhold its ending.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

mod socket;

use std::time::Duration;

use af_core::test_util::trace::Capture;
use afr_executor::Ending;
use serde_json::json;
use tokio::sync::Notify;

use crate::catalog::{GIT, SHELL};
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::testing::{call_in, hosted, offered};

use super::{EVENT_ENDING_DEADLINE, EVENT_TIMED_OUT};
use socket::{PATIENCE, pair};

// Value: protects=both public command tools finish when only the ending is lost;
// fails_when=the runner relies solely on the executor's process timeout;
// seam=scripted socket peer with the real Client and native lost token;
// no_claim=the closed connection establishes remote process death.
#[tokio::test]
async fn silent_endings_interrupt_commands_and_fence_later_calls() {
    for (entry, arguments) in [
        (&SHELL, json!({"command":"sleep 60", "timeout_ms":100})),
        (&GIT, json!({"args":["status"], "timeout_ms":100})),
    ] {
        let capture = Capture::install();
        let (witness, mut peer) = pair();
        let (catalog, _sent) = hosted();
        let selection = catalog.select(&[entry.name()]).expect("offered command");
        let lease = Lease::default();
        let drive = async {
            let request = peer.spawn(&witness).await;
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(6)).await;
            tokio::time::resume();
            peer.closed().await;
            request
        };
        let (output, request) = tokio::join!(
            call_in(
                offered(&selection, entry),
                &witness,
                &lease,
                arguments.clone()
            ),
            drive
        );
        assert_eq!(request.pointer("/params/timeout_ms"), Some(&json!(100)));
        assert_eq!(
            output.text,
            "partial\nProcess interrupted before its ending arrived"
        );
        assert_eq!(
            (output.exit_code, output.error_code),
            (None, Some(ToolErrorCode::Interrupted))
        );
        let later = tokio::time::timeout(
            PATIENCE,
            call_in(offered(&selection, entry), &witness, &lease, arguments),
        )
        .await
        .expect("invalid link refuses without waiting for the peer");
        assert_eq!(
            (later.exit_code, later.error_code),
            (None, Some(ToolErrorCode::SandboxUnavailable))
        );
        assert_deadline(&capture);
    }
}

fn assert_deadline(capture: &Capture) {
    let event = capture.only(EVENT_ENDING_DEADLINE);
    assert_eq!(event.level, tracing::Level::WARN);
    assert_eq!(event.field("error_code"), Some("interrupted"));
    assert_eq!(event.field("process_id"), Some("7"));
    assert_eq!(event.field("timeout_ms"), Some("100"));
    assert_eq!(event.field("runner_wait_ms"), Some("5100"));
    assert!(event.field("lease_id").is_some());
    assert!(
        event
            .fields
            .values()
            .all(|value| !value.contains("sleep 60"))
    );
}

// Value: protects=executor cleanup may report its real ending after command time;
// fails_when=the runner omits cleanup grace or claims an unknown result is killed;
// seam=real Client over a scripted socket; no_claim=kernel process-group cleanup.
#[tokio::test]
async fn an_ending_during_cleanup_preserves_the_confirmed_timeout_without_a_second_kill() {
    let capture = Capture::install();
    let (witness, mut peer) = pair();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[SHELL.name()]).expect("offered shell");
    let lease = Lease::default();
    let drive = async {
        peer.spawn(&witness).await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(3)).await;
        tokio::time::resume();
        peer.ended(Ending::TimedOut).await;
    };
    let arguments = json!({"command":"sleep 60", "timeout_ms":100});
    let (output, ()) = tokio::join!(
        call_in(offered(&selection, &SHELL), &witness, &lease, arguments),
        drive
    );
    assert_eq!(output.text, "partial\nProcess timed out after 100 ms");
    assert_eq!(
        (output.exit_code, output.error_code),
        (None, Some(ToolErrorCode::TimedOut))
    );
    assert_eq!(
        capture.only(EVENT_TIMED_OUT).field("timeout_ms"),
        Some("100")
    );
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_ENDING_DEADLINE))
    );
    peer.responsive(&witness.client).await;
}

// Value: protects=cancelling the caller before its guard still requests one kill;
// fails_when=the bounded wait retains a detached process or delays cancellation;
// seam=real Client and native abandonment hook; no_claim=acknowledged remote death.
#[tokio::test]
async fn caller_cancellation_keeps_native_cleanup_and_the_connection_usable() {
    let capture = Capture::install();
    let (witness, mut peer) = pair();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[SHELL.name()]).expect("offered shell");
    let lease = Lease::default();
    let cancel = Notify::new();
    let drive = async {
        peer.spawn(&witness).await;
        cancel.notify_one();
        peer.killed().await;
    };
    let arguments = json!({"command":"sleep 60", "timeout_ms":100});
    let call = async {
        tokio::select! {
            output = call_in(offered(&selection, &SHELL), &witness, &lease, arguments) => Some(output),
            () = cancel.notified() => None,
        }
    };
    let (cancelled, ()) = tokio::join!(call, drive);
    assert!(cancelled.is_none(), "caller cancelled before an ending");
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_ENDING_DEADLINE))
    );
    peer.responsive(&witness.client).await;
}
