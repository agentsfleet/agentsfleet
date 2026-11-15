//! Unusable policy input stops work rather than retaining an earlier permission.

use super::{Assignment, Heartbeat, tests::probe};
use crate::{
    error,
    halt::Halt,
    holds::Holds,
    test_support::{self, Answer, drain, plane},
};
use af_core::test_util::trace::Capture;
use af_stream::{Budget, encode};
use af_wire::protocol::StreamKind;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn closed_or_invalid_assignment_stream_never_advertises_a_worker() -> crate::Result<()> {
    for fault in ["closed", "shape", "wrong_frame"] {
        let capture = Capture::install();
        let (mut plane, mut calls) = plane(|_| Answer::Stall);
        let (updates, receiver) = watch::channel(None);
        plane.stream.assignments = receiver;
        if fault != "closed" {
            updates.send_replace(Some(Arc::new(assignment_frame(fault).await?)));
        }
        let probe = probe();
        let holds = Holds::start(test_support::clock());
        let (published, watching) = watch::channel(Assignment::initial());
        let halt = Halt::new(CancellationToken::new());
        if fault == "closed" {
            drop(updates);
        }
        test_support::bounded(
            "invalid assignment shutdown",
            Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt),
        )
        .await;
        assert!(halt.running().is_cancelled());
        assert!(!watching.borrow().takes_work(0));
        assert_eq!(drain(&mut calls).len(), 0);
        if fault != "closed" {
            assert!(
                capture
                    .only("runner_assignment_refused")
                    .field("error_code")
                    .is_some()
            );
        }
        holds.shutdown().await;
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_permanent_heartbeat_failure_stops_the_runner_without_retrying() {
    let (plane, mut calls) = plane(|_| Answer::Fail(error::config("unusable native heartbeat")));
    let probe = probe();
    let holds = Holds::start(test_support::clock());
    let (published, watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    test_support::bounded(
        "permanent heartbeat shutdown",
        Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt),
    )
    .await;
    assert!(halt.running().is_cancelled());
    assert!(!halt.token_refused());
    assert!(!watching.borrow().takes_work(0));
    assert_eq!(drain(&mut calls).len(), 1);
    holds.shutdown().await;
}

async fn assignment_frame(fault: &str) -> crate::Result<af_stream::OwnedFrame> {
    let body = if fault == "shape" {
        serde_json::json!("not an assignment")
    } else {
        serde_json::json!({"type":"pong","request_id":1})
    };
    let frame = encode(
        &body,
        &Budget::default(),
        StreamKind::Control,
        &CancellationToken::new(),
        tokio::time::Instant::now() + Duration::from_secs(1),
    )
    .await?;
    if fault == "wrong_frame" {
        assert!(matches!(
            frame.decode::<af_wire::protocol::Down<'_>>()?,
            af_wire::protocol::Down::Pong { .. }
        ));
    }
    Ok(frame)
}
