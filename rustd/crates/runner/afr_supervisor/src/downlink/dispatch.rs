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
                let registration = self
                    .running
                    .offer(id.clone(), self.early_cancel.remove(&id))?;
                let offer = Offered {
                    frame,
                    registration,
                    session: self.stop.clone(),
                };
                // Credits already reserve space. Overflow is a protocol failure, never a dropped grant.
                self.offers
                    .try_send(offer)
                    .map_err(crate::error::stream_protocol)?;
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
}
