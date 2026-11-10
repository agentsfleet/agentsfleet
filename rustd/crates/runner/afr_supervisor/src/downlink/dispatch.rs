//! Replies are correlated before ownership moves; unsolicited control never waits for a worker.

use std::{collections::HashSet, sync::Arc};

use af_core::id::Uuid7;
use af_stream::OwnedFrame;
use af_wire::{
    lease::TaggedLease,
    protocol::{Down, LEASES_MAX, StreamKind},
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::{Offered, pending::Requests, running::Running};
use crate::{Result, error::ErrorKind};

pub(crate) struct Dispatch<'a> {
    pub(crate) requests: &'a Requests,
    pub(crate) running: &'a Arc<Running>,
    pub(crate) offers: &'a mpsc::Sender<Offered>,
    pub(crate) offer_budget: &'a af_stream::Budget,
    pub(crate) assignments: &'a watch::Sender<Option<Arc<OwnedFrame>>>,
    pub(crate) stop: &'a CancellationToken,
    pub(crate) early_cancel: HashSet<Uuid7>,
}

/// A graceful drain asks for another connection; a coded go-away ends this process.
#[derive(Debug)]
pub(crate) enum Progress {
    Continue,
    Reconnect,
    Halt(af_core::error_code::ErrorCode),
}

impl Dispatch<'_> {
    pub(crate) fn frame(&mut self, kind: StreamKind, frame: OwnedFrame) -> Result<Progress> {
        let message: Down<'_> = frame.decode()?;
        if message.stream_kind() != kind {
            return Err(ErrorKind::StreamProtocol.into());
        }
        match message {
            Down::Offer { ref lease } => {
                let id: &str = match lease {
                    TaggedLease::Fleet(lease) => lease.lease_id.as_ref(),
                    TaggedLease::Authoring(lease) => lease.lease_id.as_str(),
                };
                let id = Uuid7::parse(id)?;
                self.offer(frame, id)?;
            }
            Down::Cancel { lease_id, .. } => {
                let id = Uuid7::parse(&lease_id)?;
                if !self.running.cancel(&id) {
                    if self.early_cancel.len() == LEASES_MAX && !self.early_cancel.contains(&id) {
                        return Ok(Progress::Reconnect);
                    }
                    self.early_cancel.insert(id);
                }
            }
            Down::Assignment { .. } => {
                self.assignments.send_replace(Some(Arc::new(frame)));
            }
            Down::GoAway { error_code: None } => return Ok(Progress::Reconnect),
            Down::GoAway {
                error_code: Some(code),
            } => {
                let code = af_core::error_code::ErrorCode::lookup(&code)
                    .ok_or(ErrorKind::StreamProtocol)?;
                return Ok(Progress::Halt(code));
            }
            Down::Welcome { .. } | Down::Prepared { .. } => {
                return Err(ErrorKind::StreamProtocol.into());
            }
            _ => {
                let pending_reply = self.requests.reply(&message)?;
                if let Some(reply) = pending_reply {
                    reply.send(frame);
                }
            }
        }
        Ok(Progress::Continue)
    }

    /// A peer closes its lanes independently, so another lane's end can overtake a terminal
    /// `go_away` already written to control. Control is read to its end within one answer
    /// bound before the session counts as lost; a timeout means control itself fell silent.
    pub(crate) async fn until_control_ends(
        &mut self,
        control: &mut mpsc::Receiver<(StreamKind, OwnedFrame)>,
        ended: Result<()>,
    ) -> Result<Progress> {
        let lost = ended
            .err()
            .unwrap_or_else(|| ErrorKind::Disconnected.into());
        if timed_out(&lost) {
            return Err(lost);
        }
        let deadline = tokio::time::Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
        loop {
            let next = tokio::select! {
                biased;
                () = self.stop.cancelled() => None,
                next = tokio::time::timeout_at(deadline, control.recv()) => next.ok().flatten(),
            };
            let Some((kind, frame)) = next else {
                return Err(lost);
            };
            match self.frame(kind, frame)? {
                Progress::Continue => {}
                ended => return Ok(ended),
            }
        }
    }

    fn offer(&mut self, mut frame: OwnedFrame, id: Uuid7) -> Result<()> {
        let cancelled = self.early_cancel.remove(&id);
        let registration = self.running.offer(id, cancelled)?;
        // The fixed lease-count bound admits at most one maximum-sized frame per registration.
        frame.try_rebudget(self.offer_budget, StreamKind::Work)?;
        let offer = Offered {
            frame,
            registration,
            session: self.stop.clone(),
        };
        self.offers
            .try_send(offer)
            .map_err(crate::error::stream_protocol)?;
        Ok(())
    }
}

fn timed_out(error: &crate::Error) -> bool {
    match error.kind() {
        ErrorKind::StreamTimeout { .. } => true,
        ErrorKind::Stream { source } => source.reason() == af_stream::Reason::Timeout,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_stream::{Budget, encode};
    use tokio::time::Instant;

    #[tokio::test]
    async fn late_cancellation_overflow_recovers_without_halting_the_runner() -> Result<()> {
        let requests = Requests::new();
        let running = Arc::new(Running::default());
        let (offers, _receiver) = mpsc::channel(LEASES_MAX);
        let (assignments, _watching) = watch::channel(None);
        let stop = CancellationToken::new();
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &running,
            offers: &offers,
            offer_budget: &Budget::default(),
            assignments: &assignments,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        for index in 1..=LEASES_MAX + 1 {
            let lease = format!("019f0000-0000-7000-8000-{index:012x}");
            let message: Down<'_> = Down::Cancel {
                lease_id: lease.into(),
                error_code: af_core::error_code::RUN_LEASE_CANCELLED.as_str().into(),
            };
            let frame = encode(
                &message,
                &Budget::default(),
                StreamKind::Control,
                &stop,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await?;
            let progress = dispatch.frame(StreamKind::Control, frame)?;
            if index <= LEASES_MAX {
                assert!(matches!(progress, Progress::Continue));
            } else {
                assert!(matches!(progress, Progress::Reconnect));
            }
        }
        assert_eq!(dispatch.early_cancel.len(), LEASES_MAX);
        assert!(!stop.is_cancelled());
        Ok(())
    }

    #[tokio::test]
    async fn a_cancel_received_before_its_offer_prevents_execution()
    -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::test_support::{Behaviour, FakeAgent, FakeEngine, Rig, bounded, daemon};
        use std::sync::atomic::Ordering;
        let rig = Rig::new(
            daemon(|_| None),
            FakeEngine::default(),
            FakeAgent::new(Behaviour::Answer),
        );
        let requests = Requests::new();
        let (offers, offered) = mpsc::channel(LEASES_MAX);
        let (assignments, _) = watch::channel(None);
        let stop = CancellationToken::new();
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &rig.lessee.plane.stream.running,
            offers: &offers,
            offer_budget: &Budget::default(),
            assignments: &assignments,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        cancel_then_offer(&mut dispatch, &stop).await?;
        assert!(dispatch.early_cancel.is_empty());
        drop(dispatch);
        drop(offers);
        let (_, watching) = watch::channel(super::super::policy::Assignment::initial());
        let pool = tokio::spawn(crate::worker_pool::serve(
            Arc::clone(&rig.lessee),
            watching,
            offered,
        ));
        bounded("cancelled wire offer removal", async {
            while !rig.lessee.plane.stream.running.snapshot().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        rig.shutdown.cancel();
        bounded("cancelled wire offer pool shutdown", pool).await?;
        assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
        Ok(())
    }

    async fn cancel_then_offer(
        dispatch: &mut Dispatch<'_>,
        stop: &CancellationToken,
    ) -> Result<()> {
        use crate::test_support::{FLEET_ID, LEASE_ID, lease};
        let messages: [Down<'_>; 2] = [
            Down::Cancel {
                lease_id: LEASE_ID.into(),
                error_code: af_core::error_code::RUN_LEASE_CANCELLED.as_str().into(),
            },
            Down::Offer {
                lease: TaggedLease::<serde_json::Value>::Fleet(Box::new(lease(LEASE_ID, FLEET_ID))),
            },
        ];
        for message in messages {
            let kind = message.stream_kind();
            let frame = encode(
                &message,
                &Budget::default(),
                kind,
                stop,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await?;
            assert!(matches!(dispatch.frame(kind, frame)?, Progress::Continue));
        }
        Ok(())
    }
}
