//! Retained worker offers cannot occupy the receive capacity needed to settle those workers.

use super::{
    Offered,
    dispatch::{Dispatch, Progress},
    pending::{Expected, Requests},
    running::Running,
};
use crate::{
    Result,
    test_support::{FLEET_ID, LEASE_ID, lease},
};
use af_core::id::Uuid7;
use af_stream::{Budget, OwnedFrame, Reader};
use af_wire::{
    lease::TaggedLease,
    protocol::{Down, LEASES_MAX, MESSAGE_BYTES_MAX, StreamKind},
};
use serde_json::json;
use std::{collections::HashSet, sync::Arc, task::Poll, time::Duration};
use tokio::{
    io::{AsyncWriteExt as _, DuplexStream},
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;

type Proof<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const SECOND: &str = "019f0000-0000-7000-8000-000000000002";
const THIRD: &str = "019f0000-0000-7000-8000-000000000003";

struct Pressure {
    requests: Arc<Requests>,
    running: Arc<Running>,
    offers: mpsc::Sender<Offered>,
    offered: mpsc::Receiver<Offered>,
    assignments: watch::Sender<Option<Arc<OwnedFrame>>>,
    retained: Budget,
    stop: CancellationToken,
}

impl Pressure {
    fn new(retained: Budget) -> Self {
        let (offers, offered) = mpsc::channel(LEASES_MAX);
        let (assignments, _) = watch::channel(None);
        Self {
            requests: Requests::new(),
            running: Arc::new(Running::default()),
            offers,
            offered,
            assignments,
            retained,
            stop: CancellationToken::new(),
        }
    }

    fn accept(&self, kind: StreamKind, frame: OwnedFrame) -> Result<()> {
        let mut dispatch = Dispatch {
            requests: &self.requests,
            running: &self.running,
            offers: &self.offers,
            offer_budget: &self.retained,
            assignments: &self.assignments,
            stop: &self.stop,
            early_cancel: HashSet::new(),
        };
        assert!(matches!(dispatch.frame(kind, frame)?, Progress::Continue));
        Ok(())
    }
}

struct Channel {
    writer: DuplexStream,
    reader: Reader<DuplexStream>,
    kind: StreamKind,
}

impl Channel {
    fn new(budget: &Budget, kind: StreamKind) -> Self {
        let (writer, input) = tokio::io::duplex(16 * 1024);
        Self {
            writer,
            reader: Reader::new(
                input,
                budget.clone(),
                kind,
                CancellationToken::new(),
                Duration::from_secs(5),
            ),
            kind,
        }
    }

    async fn send(&mut self, body: &[u8]) -> Proof<()> {
        self.writer.write_u32(u32::try_from(body.len())?).await?;
        self.writer.write_all(body).await?;
        Ok(())
    }

    async fn dispatch(&mut self, pressure: &Pressure) -> Proof<()> {
        let frame = tokio::time::timeout(Duration::from_secs(1), self.reader.next())
            .await??
            .ok_or("the peer closed before its frame")?;
        pressure.accept(self.kind, frame)?;
        Ok(())
    }
}

fn offer(id: &str) -> Proof<Vec<u8>> {
    let message: Down<'_> = Down::Offer {
        lease: TaggedLease::<serde_json::Value>::Fleet(Box::new(lease(id, FLEET_ID))),
    };
    Ok(serde_json::to_vec(&message)?)
}

#[tokio::test(start_paused = true)]
async fn retained_offers_and_a_waiting_work_reader_do_not_block_report_acknowledgements()
-> Proof<()> {
    let [first, second, third] = [offer(LEASE_ID)?, offer(SECOND)?, offer(THIRD)?];
    let pressure = Pressure::new(Budget::new(0, LEASES_MAX * MESSAGE_BYTES_MAX)?);
    let mut report = pressure
        .requests
        .register(Expected::Report(Uuid7::parse(LEASE_ID)?))
        .await?;
    let answer = serde_json::to_vec(&json!({"type":"report_ack", "request_id":report.id,
        "lease_id":LEASE_ID, "answer":{"status":"ok","response":{"ok":true}}}))?;
    let input = Budget::new(0, first.len() + second.len() + answer.len())?;
    let mut work = Channel::new(&input, StreamKind::Work);
    let mut reports = Channel::new(&input, StreamKind::Reports);
    for body in [&first, &second] {
        work.send(body).await?;
        work.dispatch(&pressure).await?;
    }
    work.writer.write_u32(u32::try_from(third.len())?).await?;
    let waiting = work.reader.next();
    tokio::pin!(waiting);
    assert!(matches!(futures_util::poll!(&mut waiting), Poll::Pending));
    reports.send(&answer).await?;
    let delivered = reports.dispatch(&pressure).await;
    assert!(
        delivered.is_ok(),
        "retained offers blocked their report acknowledgement: {delivered:?}"
    );
    delivered?;
    let acknowledged = tokio::time::timeout(Duration::from_secs(1), report.receive()).await??;
    assert_eq!(acknowledged.as_bytes(), answer);
    assert_eq!(pressure.offered.len(), 2);
    assert_eq!(pressure.running.snapshot().len(), 2);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn folded_input_and_reconnected_offers_share_a_bounded_retention_allowance() -> Proof<()> {
    let retained = Budget::new(0, LEASES_MAX * MESSAGE_BYTES_MAX)?;
    let available = retained.available(StreamKind::Work);
    let pressure = Pressure::new(retained.clone());
    let input = Budget::new(0, 16 * 1024)?;
    let mut work = Channel::new(&input, StreamKind::Work);
    work.send(&offer(LEASE_ID)?).await?;
    work.dispatch(&pressure).await?;
    let held = pressure.offered.len();
    let mut folded = pressure
        .requests
        .register(Expected::Folds(Uuid7::parse(LEASE_ID)?))
        .await?;
    let answer = serde_json::to_vec(&json!({"type":"folded", "request_id":folded.id,
        "lease_id":LEASE_ID, "answer":{"status":"ok","response":{"inputs":[]}}}))?;
    work.send(&answer).await?;
    work.dispatch(&pressure).await?;
    let response = tokio::time::timeout(Duration::from_secs(1), folded.receive()).await??;
    assert!(matches!(
        response.decode::<Down<'_>>()?,
        Down::Folded { .. }
    ));
    assert_eq!(pressure.offered.len(), held);
    pressure.running.new_session();
    let mut reconnected = Channel::new(&input, StreamKind::Work);
    reconnected.send(&offer(SECOND)?).await?;
    reconnected.dispatch(&pressure).await?;
    assert_eq!(pressure.offered.len(), 2);
    assert_eq!(pressure.running.snapshot().len(), 2);
    assert!(retained.available(StreamKind::Work) < available);
    drop(response);
    drop(pressure);
    assert_eq!(retained.available(StreamKind::Work), available);
    assert_eq!(input.available(StreamKind::Work), 16 * 1024);
    Ok(())
}

#[tokio::test]
async fn an_offer_without_retention_capacity_releases_its_registration_and_input_charge()
-> Proof<()> {
    let pressure = Pressure::new(Budget::new(0, 0)?);
    let input = Budget::new(0, 16 * 1024)?;
    let mut work = Channel::new(&input, StreamKind::Work);
    work.send(&offer(LEASE_ID)?).await?;
    let failure = work
        .dispatch(&pressure)
        .await
        .err()
        .ok_or("offer unexpectedly admitted")?;
    let failure = failure.downcast::<crate::Error>()?;
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        std::error::Error::source(failure.as_ref())
            .and_then(|source| source.downcast_ref::<af_stream::Error>())
            .is_some_and(|source| source.reason() == af_stream::Reason::Protocol)
    );
    assert_eq!(pressure.running.snapshot(), [] as [Uuid7; 0]);
    assert!(pressure.offered.is_empty());
    assert_eq!(input.available(StreamKind::Work), 16 * 1024);
    Ok(())
}
