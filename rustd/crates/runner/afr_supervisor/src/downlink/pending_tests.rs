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
        Offered,
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
    assignments: watch::Sender<Option<Arc<OwnedFrame>>>,
    offers: mpsc::Sender<Offered>,
    _offered: mpsc::Receiver<Offered>,
}

impl Harness {
    fn new() -> Self {
        let running = Arc::new(Running::default());
        let (current, session) = watch::channel(None);
        let (assignments, watching) = watch::channel(None);
        let stream = StreamHandle::new(session, Arc::clone(&running), watching);
        let (offers, offered) = mpsc::channel(af_wire::protocol::LEASES_MAX);
        Self {
            plane: ControlPlane::new(Box::new(NoHttp), stream),
            current,
            running,
            assignments,
            offers,
            _offered: offered,
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
            offers: &self.offers,
            assignments: &self.assignments,
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

fn lease() -> Uuid7 {
    Uuid7::parse(crate::test_util::LEASE_ID).expect("fixture UUIDv7")
}

fn folded(request: &Value, text: &str) -> Value {
    json!({"type":"folded","request_id":(*request.pointer("/request_id").expect("native response field")),"lease_id":(*request.pointer("/lease_id").expect("native response field")),
        "answer":{"status":"ok","response":{"inputs":[{"event_id":crate::test_util::FLEET_ID,
            "sequence":1,"text":text}]}}})
}

fn renewed(request: &Value, through: u32) -> Value {
    json!({"type":"renewed","request_id":(*request.pointer("/request_id").expect("native response field")),
        "leases":[{"lease_id":(*request.pointer("/leases/0/lease_id").expect("native response field")),"answer":{"status":"ok",
            "response":{"lease_expires_at":"2030-01-01T00:00:00Z","recorded_through":through}}}]})
}

#[tokio::test(start_paused = true)]
async fn a_late_fold_answer_cannot_complete_the_next_caller_after_cancellation() -> Result<()> {
    let harness = Harness::new();
    let mut peer = harness.connect();
    let lease = lease();
    let mut abandoned = Box::pin(harness.plane.folds(&lease, 7, 0));
    assert!(matches!(futures_util::poll!(&mut abandoned), Poll::Pending));
    let previous = peer.next(StreamKind::Work).await;
    drop(abandoned);
    peer.released();
    let mut current = Box::pin(harness.plane.folds(&lease, 7, 0));
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
    let lease = lease();
    let usage = RenewRequest {
        requests: af_wire::usage::RequestLines::default(),
    };
    let mut abandoned = Box::pin(harness.plane.renew(&lease, &usage));
    assert!(matches!(futures_util::poll!(&mut abandoned), Poll::Pending));
    let previous = peer.next(StreamKind::Control).await;
    drop(abandoned);
    peer.released();
    let mut current = Box::pin(harness.plane.renew(&lease, &usage));
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
    let lease = lease();
    let mut previous = Box::pin(harness.plane.folds(&lease, 7, 0));
    assert!(matches!(futures_util::poll!(&mut previous), Poll::Pending));
    let old_request = previous_peer.next(StreamKind::Work).await;
    previous_peer.disconnect();
    let failure = previous
        .await
        .expect_err("disconnect fails the original caller");
    assert!(failure.is_retryable());
    previous_peer.released();
    let mut peer = harness.connect();
    let mut current = Box::pin(harness.plane.folds(&lease, 7, 0));
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
