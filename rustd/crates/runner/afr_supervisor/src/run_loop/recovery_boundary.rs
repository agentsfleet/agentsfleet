//! The capture future outlives a cancelled, panicking or vanished agent caller.

use std::sync::OnceLock;

use afr_agent::{RecoveryBoundarySink, RecoverySave, RecoverySkip, RecoverySnapshot};
use afr_sandbox::Sandbox;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{Run, recovery::State};
use crate::{error, recovery::RecoveryResources, report::Ending};

#[derive(Debug)]
struct Job {
    snapshot: RecoverySnapshot,
    reply: oneshot::Sender<afr_agent::Result<RecoverySave>>,
}

/// A file run cannot claim API-only state while its workspace is building.
#[derive(Debug, Clone, Copy)]
pub(super) enum Workspace<'a> {
    ApiOnly,
    Building {
        cell: &'a OnceLock<Box<dyn Sandbox>>,
        recovery: &'a OnceLock<State>,
    },
}

#[derive(Debug)]
pub(super) struct Sink<'a> {
    jobs: mpsc::Sender<Job>,
    active: Mutex<()>,
    workspace: Workspace<'a>,
    resources: &'a RecoveryResources,
}

#[async_trait::async_trait]
impl RecoveryBoundarySink for Sink<'_> {
    fn workspace(&self) -> Result<af_wire::run::TurnWorkspace, RecoverySkip> {
        match self.workspace {
            Workspace::ApiOnly => Ok(af_wire::run::TurnWorkspace::ApiOnly),
            Workspace::Building { cell, .. } => {
                let sandbox = cell.get().ok_or(RecoverySkip::WorkspaceBuilding)?;
                let toolbox = sandbox.toolbox_digest().ok_or(RecoverySkip::Unavailable)?;
                Ok(af_wire::run::TurnWorkspace::Files {
                    toolbox_digest: toolbox.to_owned(),
                })
            }
        }
    }

    fn try_reserve(
        &self,
        task: bool,
    ) -> Result<Option<afr_agent::FileRecoveryPermit>, RecoverySkip> {
        match self.workspace {
            Workspace::ApiOnly if !task => return Ok(None),
            Workspace::Building { cell, .. } if cell.get().is_none() => {
                return Err(RecoverySkip::WorkspaceBuilding);
            }
            Workspace::ApiOnly | Workspace::Building { .. } => {}
        }
        self.resources
            .try_file()
            .map(Some)
            .ok_or(RecoverySkip::BusyStorage)
    }

    async fn save(&self, snapshot: RecoverySnapshot) -> afr_agent::Result<RecoverySave> {
        let Ok(_active) = self.active.try_lock() else {
            return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
        };
        let (reply, receive) = oneshot::channel();
        self.jobs
            .send(Job { snapshot, reply })
            .await
            .map_err(|source| {
                afr_agent::Error::checkpoint(error::boundary("capture owner closed", source))
            })?;
        receive.await.map_err(|source| {
            afr_agent::Error::checkpoint(error::boundary("capture owner lost its reply", source))
        })?
    }
}

pub(super) struct BoundaryOwner<'a, 'run> {
    pub(super) run: &'a Run<'run>,
    pub(super) workspace: Workspace<'a>,
    pub(super) stop: CancellationToken,
    jobs: mpsc::Receiver<Job>,
    pub(super) turn: crate::recovery::NativeTurn<'a>,
}

impl<'a, 'run> BoundaryOwner<'a, 'run> {
    pub(super) fn new(run: &'a Run<'run>, workspace: Workspace<'a>) -> (Sink<'a>, Self) {
        let (send, jobs) = mpsc::channel(1);
        (
            Sink {
                jobs: send,
                active: Mutex::new(()),
                workspace,
                resources: &run.lessee.recovery,
            },
            Self {
                run,
                turn: crate::recovery::NativeTurn::new(
                    &run.lessee.plane,
                    &run.ids.run,
                    run.payload.fencing_token,
                    &run.lessee.recovery.transfers,
                ),
                workspace,
                stop: run.interrupt.child_token(),
                jobs,
            },
        )
    }

    pub(super) async fn beside(mut self, work: impl Future<Output = Ending>) -> Ending {
        tokio::pin!(work);
        let ending = loop {
            tokio::select! {
                biased;
                ending = &mut work => break ending,
                job = self.jobs.recv() => {
                    let Some(job) = job else { break work.await };
                    // Dropping the reply receiver never drops this operation.
                    // Poll work too: its stop grace can end while upload waits.
                    let stop = self.stop.clone();
                    let capture = Box::pin(self.capture(job.snapshot));
                    tokio::pin!(capture);
                    let completed = tokio::select! {
                        biased;
                        result = &mut capture => { let _gone = job.reply.send(result); None }
                        ending = &mut work => {
                            stop.cancel();
                            let result = capture.await;
                            let _gone = job.reply.send(result);
                            Some(ending)
                        }
                    };
                    if let Some(ending) = completed { break ending; }
                }
            }
        };
        self.jobs.close();
        self.stop.cancel();
        while let Ok(job) = self.jobs.try_recv() {
            let _gone = job
                .reply
                .send(Ok(RecoverySave::Skipped(RecoverySkip::Unavailable)));
        }
        ending
    }
}
