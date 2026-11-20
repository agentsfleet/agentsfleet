//! Replies are correlated before ownership moves; unsolicited control never waits for a worker.

use std::{collections::HashSet, sync::Arc};

use af_core::id::Uuid7;
use af_stream::OwnedFrame;
use af_wire::{
    protocol::{Down, RUNS_MAX, StreamKind},
    run::TaggedRun,
};
use garde::Validate as _;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::{Assigned, pending::Requests, running::Running};
use crate::{Result, error::ErrorKind};

pub(crate) struct Dispatch<'a> {
    pub(crate) requests: &'a Requests,
    pub(crate) running: &'a Arc<Running>,
    pub(crate) assignments: &'a mpsc::Sender<Assigned>,
    pub(crate) assignment_budget: &'a af_stream::Budget,
    pub(crate) policies: &'a watch::Sender<Option<Arc<OwnedFrame>>>,
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
        if matches!(message, Down::TurnCheckpoint { .. }) {
            af_wire::run::TurnMessageSize {
                bytes: frame.as_bytes().len(),
            }
            .validate()?;
        }
        if message.stream_kind() != kind {
            return Err(ErrorKind::StreamProtocol.into());
        }
        match message {
            Down::Run { ref run } => {
                let TaggedRun::Fleet(run) = run;
                let id = Uuid7::parse(&run.run_id)?;
                self.assign(frame, id)?;
            }
            Down::Cancel { run_id, error_code } => {
                let id = Uuid7::parse(&run_id)?;
                // `UZ-RUN-022` is the run's durable cancel mark: a person's Stop.
                // A reconciliation's Cancel names another code and is no Stop.
                let stopped = error_code == af_core::error_code::RUN_CANCELLED.as_str();
                if !self.running.cancel(&id, stopped) {
                    if self.early_cancel.len() == RUNS_MAX && !self.early_cancel.contains(&id) {
                        return Ok(Progress::Reconnect);
                    }
                    self.early_cancel.insert(id);
                }
            }
            Down::Policy { .. } => {
                self.policies.send_replace(Some(Arc::new(frame)));
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

    fn assign(&mut self, mut frame: OwnedFrame, id: Uuid7) -> Result<()> {
        let cancelled = self.early_cancel.remove(&id);
        let registration = self.running.assign(id, cancelled)?;
        // The fixed run-count bound admits at most one maximum-sized frame per registration.
        frame.try_rebudget(self.assignment_budget, StreamKind::Work)?;
        let assignment = Assigned {
            frame,
            registration,
            session: self.stop.clone(),
        };
        self.assignments
            .try_send(assignment)
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
        let (assignments, _receiver) = mpsc::channel(RUNS_MAX);
        let (policies, _watching) = watch::channel(None);
        let stop = CancellationToken::new();
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &running,
            assignments: &assignments,
            assignment_budget: &Budget::default(),
            policies: &policies,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        for index in 1..=RUNS_MAX + 1 {
            let run = format!("019f0000-0000-7000-8000-{index:012x}");
            let message: Down<'_> = Down::Cancel {
                run_id: run.into(),
                error_code: af_core::error_code::RUN_CANCELLED.as_str().into(),
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
            if index <= RUNS_MAX {
                assert!(matches!(progress, Progress::Continue));
            } else {
                assert!(matches!(progress, Progress::Reconnect));
            }
        }
        assert_eq!(dispatch.early_cancel.len(), RUNS_MAX);
        assert!(!stop.is_cancelled());
        Ok(())
    }

    #[tokio::test]
    async fn a_cancel_received_before_its_assignment_prevents_execution()
    -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::test_support::{Behaviour, FakeAgent, FakeEngine, Rig, bounded, daemon};
        use std::sync::atomic::Ordering;
        let rig = Rig::new(
            daemon(|_| None),
            FakeEngine::default(),
            FakeAgent::new(Behaviour::Answer),
        );
        let requests = Requests::new();
        let (assignments, assigned) = mpsc::channel(RUNS_MAX);
        let (policies, _) = watch::channel(None);
        let stop = CancellationToken::new();
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &rig.lessee.plane.stream.running,
            assignments: &assignments,
            assignment_budget: &Budget::default(),
            policies: &policies,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        cancel_then_assign(&mut dispatch, &stop).await?;
        assert!(dispatch.early_cancel.is_empty());
        drop(dispatch);
        drop(assignments);
        let (_, watching) = watch::channel(super::super::policy::Assignment::initial());
        let pool = tokio::spawn(crate::worker_pool::serve(
            Arc::clone(&rig.lessee),
            watching,
            assigned,
        ));
        bounded("cancelled wire assignment removal", async {
            while !rig.lessee.plane.stream.running.snapshot().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        rig.shutdown.cancel();
        bounded("cancelled wire assignment pool shutdown", pool).await?;
        assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn only_a_cancel_naming_the_cancel_mark_is_a_stop()
    -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::test_support::RUN_ID;
        let running = Arc::new(Running::default());
        let requests = Requests::new();
        let (assignments, _assigned) = mpsc::channel(RUNS_MAX);
        let (policies, _) = watch::channel(None);
        let stop = CancellationToken::new();
        let mut dispatch = Dispatch {
            requests: &requests,
            running: &running,
            assignments: &assignments,
            assignment_budget: &Budget::default(),
            policies: &policies,
            stop: &stop,
            early_cancel: HashSet::new(),
        };
        let run = Uuid7::parse(RUN_ID)?;
        for (code, stopped) in [
            (af_core::error_code::RUN_LOST, false),
            (af_core::error_code::RUN_CANCELLED, true),
        ] {
            let registration = running.assign(run.clone(), false)?;
            let message: Down<'_> = Down::Cancel {
                run_id: RUN_ID.into(),
                error_code: code.as_str().into(),
            };
            let kind = message.stream_kind();
            let frame = encode(
                &message,
                &Budget::default(),
                kind,
                &stop,
                Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
            )
            .await?;
            assert!(matches!(dispatch.frame(kind, frame)?, Progress::Continue));
            assert!(registration.stop.is_cancelled(), "{code}");
            assert_eq!(running.stopped(&run), stopped, "{code}");
        }
        Ok(())
    }

    async fn cancel_then_assign(
        dispatch: &mut Dispatch<'_>,
        stop: &CancellationToken,
    ) -> Result<()> {
        use crate::test_support::{FLEET_ID, RUN_ID, payload};
        let messages: [Down<'_>; 2] = [
            Down::Cancel {
                run_id: RUN_ID.into(),
                error_code: af_core::error_code::RUN_CANCELLED.as_str().into(),
            },
            Down::Run {
                run: TaggedRun::Fleet(Box::new(payload(RUN_ID, FLEET_ID))),
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
