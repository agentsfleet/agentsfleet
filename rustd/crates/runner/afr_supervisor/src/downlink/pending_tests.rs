//! Cancelled and disconnected callers release correlation and cannot receive a later session's answer.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "unit preconditions fail loudly"
)]

use super::*;
use crate::{
    client::{Call, ControlPlane, RunnerApi},
    downlink::{
        Assigned,
        dispatch::{Dispatch, Progress},
        running::Running,
    },
    uplink::{
        SessionSender, StreamHandle,
        queues::{Queues, Receivers},
    },
};
use af_core::id::Uuid7;
use af_stream::Budget;
use af_wire::{protocol::StreamKind, report::RenewRequest};
use bytes::Bytes;
use serde_json::{Value, json};
use std::{collections::HashSet, task::Poll};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct NoHttp;

#[test]
fn turn_replies_require_run_operation_kind_and_matching_success_variant() -> serde_json::Result<()>
{
    use af_wire::run::{CheckpointOperationId, TurnCheckpointKind};
    let run = "01900000-0000-7000-8000-000000000001";
    let operation = "01900000-0000-7000-8000-000000000002";
    let other = "01900000-0000-7000-8000-000000000003";
    let expected = Expected::TurnCheckpoint {
        run: serde_json::from_value(json!(run))?,
        operation: CheckpointOperationId(operation.into()),
        kind: TurnCheckpointKind::PinRelease,
    };
    let original = json!({"type":"turn_checkpoint","request_id":2,"run_id":run,
        "operation_id":operation,"kind":"pin_release","answer":{"status":"ok","response":{"kind":"pin_released"}}});
    let accept = |value: &Value| -> serde_json::Result<bool> {
        let bytes = serde_json::to_string(value)?;
        Ok(expected.accepts(&serde_json::from_str::<Down<'_>>(&bytes)?))
    };
    assert!(accept(&original)?);
    for (key, wrong) in [
        ("run_id", other),
        ("operation_id", other),
        ("kind", "abandon"),
    ] {
        let mut changed = original.clone();
        *field(&mut changed, &format!("/{key}"))? = json!(wrong);
        assert!(!accept(&changed)?);
    }
    let mut changed = original;
    *field(&mut changed, "/answer/response/kind")? = json!("abandoned");
    assert!(!accept(&changed)?);
    *field(&mut changed, "/answer")? = json!({"status":"refused","error_code":"UZ-RUN-027","detail":"Unavailable","retryable":true});
    assert!(accept(&changed)?);
    *field(&mut changed, "/operation_id")? = json!(other);
    assert!(!accept(&changed)?);
    Ok(())
}

fn field<'a>(value: &'a mut Value, pointer: &str) -> serde_json::Result<&'a mut Value> {
    value.pointer_mut(pointer).ok_or_else(|| {
        <serde_json::Error as serde::de::Error>::custom("complete native correlation fixture")
    })
}

#[async_trait::async_trait]
impl RunnerApi for NoHttp {
    async fn send(&self, _call: Call) -> Result<Bytes> {
        panic!("folds and renewals must stay on the native uplink")
    }
}

struct Harness {
    plane: ControlPlane,
    current: watch::Sender<Option<Arc<SessionSender>>>,
    running: Arc<Running>,
    policies: watch::Sender<Option<Arc<OwnedFrame>>>,
    assignments: mpsc::Sender<Assigned>,
    _assigned: mpsc::Receiver<Assigned>,
}

impl Harness {
    fn new() -> Self {
        let running = Arc::new(Running::default());
        let (current, session) = watch::channel(None);
        let (policies, watching) = watch::channel(None);
        let stream = StreamHandle::new(session, Arc::clone(&running), watching);
        let (assignments, assigned) = mpsc::channel(af_wire::protocol::RUNS_MAX);
        Self {
            plane: ControlPlane::new(Box::new(NoHttp), stream),
            current,
            running,
            policies,
            assignments,
            _assigned: assigned,
        }
    }

    fn connect(&self) -> Peer {
        let (queues, outgoing) = Queues::new();
        let sender = Arc::new(SessionSender {
            requests: Requests::new(),
            queues,
            budget: Budget::default(),
            stop: CancellationToken::new(),
        });
        self.current.send_replace(Some(Arc::clone(&sender)));
        Peer { sender, outgoing }
    }

    async fn deliver(&self, peer: &Peer, kind: StreamKind, value: &Value) -> Result<()> {
        let frame = af_stream::encode(
            value,
            &Budget::default(),
            kind,
            &CancellationToken::new(),
            Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
        )
        .await?;
        let mut dispatch = Dispatch {
            requests: &peer.sender.requests,
            running: &self.running,
            assignments: &self.assignments,
            assignment_budget: &Budget::default(),
            policies: &self.policies,
            stop: &peer.sender.stop,
            early_cancel: HashSet::new(),
        };
        assert!(matches!(dispatch.frame(kind, frame)?, Progress::Continue));
        Ok(())
    }
}

struct Peer {
    sender: Arc<SessionSender>,
    outgoing: Receivers,
}

impl Peer {
    async fn next(&mut self, kind: StreamKind) -> Value {
        let queue = match kind {
            StreamKind::Work => &mut self.outgoing.work,
            StreamKind::Control => &mut self.outgoing.priority,
            _ => panic!("this test drives only folds and renewals"),
        };
        queue
            .recv()
            .await
            .expect("the real uplink queued its request")
            .frame
            .decode()
            .expect("strict outbound JSON")
    }

    fn released(&self) {
        assert_eq!(self.sender.requests.entries.lock().len(), 0);
        assert_eq!(
            self.sender.requests.capacity.available_permits(),
            PENDING_MAX
        );
    }

    fn disconnect(&self) {
        self.sender.stop.cancel();
        self.sender.requests.close();
    }
}

fn payload() -> Uuid7 {
    Uuid7::parse(crate::test_util::RUN_ID).expect("fixture UUIDv7")
}

fn folded(request: &Value, text: &str) -> Value {
    json!({"type":"folded","request_id":(*request.pointer("/request_id").expect("native response field")),"run_id":(*request.pointer("/run_id").expect("native response field")),
        "answer":{"status":"ok","response":{"inputs":[{"turn_id":crate::test_util::FLEET_ID,
            "sequence":1,"text":text}]}}})
}

fn renewed(request: &Value, through: u32) -> Value {
    json!({"type":"renewed","request_id":(*request.pointer("/request_id").expect("native response field")),
        "runs":[{"run_id":(*request.pointer("/runs/0/run_id").expect("native response field")),"answer":{"status":"ok",
            "response":{"run_expires_at":"2030-01-01T00:00:00Z","recorded_through":through}}}]})
}

#[tokio::test(start_paused = true)]
async fn a_late_fold_answer_cannot_complete_the_next_caller_after_cancellation() -> Result<()> {
    let harness = Harness::new();
    let mut peer = harness.connect();
    let run = payload();
    let mut abandoned = Box::pin(harness.plane.folds(&run, 7, 0));
    assert!(matches!(futures_util::poll!(&mut abandoned), Poll::Pending));
    let previous = peer.next(StreamKind::Work).await;
    drop(abandoned);
    peer.released();
    let mut current = Box::pin(harness.plane.folds(&run, 7, 0));
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    let request = peer.next(StreamKind::Work).await;
    assert_ne!(
        (*request
            .pointer("/request_id")
            .expect("native response field")),
        (*previous
            .pointer("/request_id")
            .expect("native response field"))
    );
    harness
        .deliver(&peer, StreamKind::Work, &folded(&previous, "abandoned"))
        .await?;
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    harness
        .deliver(&peer, StreamKind::Work, &folded(&request, "current"))
        .await?;
    let body = current.await?;
    let result: af_wire::fold::FoldResponse<'_> = body.decode()?;
    assert_eq!(
        result.inputs.first().expect("one folded input").text,
        "current"
    );
    peer.released();
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_late_renewal_answer_cannot_acknowledge_a_new_callers_usage() -> Result<()> {
    let harness = Harness::new();
    let mut peer = harness.connect();
    let run = payload();
    let usage = RenewRequest {
        requests: af_wire::usage::RequestLines::default(),
    };
    let mut abandoned = Box::pin(harness.plane.renew(&run, &usage));
    assert!(matches!(futures_util::poll!(&mut abandoned), Poll::Pending));
    let previous = peer.next(StreamKind::Control).await;
    drop(abandoned);
    peer.released();
    let mut current = Box::pin(harness.plane.renew(&run, &usage));
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    let request = peer.next(StreamKind::Control).await;
    assert_ne!(
        (*request
            .pointer("/request_id")
            .expect("native response field")),
        (*previous
            .pointer("/request_id")
            .expect("native response field"))
    );
    harness
        .deliver(&peer, StreamKind::Control, &renewed(&previous, 99))
        .await?;
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    harness
        .deliver(&peer, StreamKind::Control, &renewed(&request, 7))
        .await?;
    assert_eq!(current.await?.recorded_through, Some(7));
    peer.released();
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn equal_request_ids_from_different_sessions_keep_separate_pending_owners() -> Result<()> {
    let harness = Harness::new();
    let mut previous_peer = harness.connect();
    let run = payload();
    let mut previous = Box::pin(harness.plane.folds(&run, 7, 0));
    assert!(matches!(futures_util::poll!(&mut previous), Poll::Pending));
    let old_request = previous_peer.next(StreamKind::Work).await;
    previous_peer.disconnect();
    let failure = previous
        .await
        .expect_err("disconnect fails the original caller");
    assert!(failure.is_retryable());
    previous_peer.released();
    let mut peer = harness.connect();
    let mut current = Box::pin(harness.plane.folds(&run, 7, 0));
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    let request = peer.next(StreamKind::Work).await;
    assert_eq!(
        (*request
            .pointer("/request_id")
            .expect("native response field")),
        (*old_request
            .pointer("/request_id")
            .expect("native response field"))
    );
    harness
        .deliver(
            &previous_peer,
            StreamKind::Work,
            &folded(&old_request, "old session"),
        )
        .await?;
    assert!(matches!(futures_util::poll!(&mut current), Poll::Pending));
    harness
        .deliver(&peer, StreamKind::Work, &folded(&request, "new session"))
        .await?;
    let body = current.await?;
    let result: af_wire::fold::FoldResponse<'_> = body.decode()?;
    assert_eq!(
        result.inputs.first().expect("one folded input").text,
        "new session"
    );
    peer.released();
    Ok(())
}
