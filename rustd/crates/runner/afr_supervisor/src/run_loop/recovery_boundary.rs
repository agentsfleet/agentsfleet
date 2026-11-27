//! The capture future outlives a cancelled, panicking or vanished agent caller.
//!
//! One publication runs per owner: a save holds the owner's one permit from
//! its pause until its commit answers, and a boundary that finds the permit
//! taken skips without pausing.

use std::sync::{Arc, OnceLock};

use afr_agent::{Publication, RecoveryBoundarySink, RecoverySave, RecoverySkip, RecoverySnapshot};
use afr_engine::Sandbox;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{Run, recovery::State, recovery_capture::Pinned};
use crate::{error, recovery::RecoveryResources, report::Ending};

#[derive(Debug)]
struct Job {
    snapshot: RecoverySnapshot,
    reply: oneshot::Sender<afr_agent::Result<RecoverySave>>,
    /// The owner's one publication, held until this save's commit answers.
    publishing: OwnedSemaphorePermit,
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
    publishing: Arc<Semaphore>,
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
                crate::recovery::Taken::turn_workspace(sandbox.environment())
                    .ok_or(RecoverySkip::Unavailable)
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
        let Ok(publishing) = Arc::clone(&self.publishing).try_acquire_owned() else {
            return Ok(RecoverySave::Skipped(RecoverySkip::BusyPublication));
        };
        let (reply, receive) = oneshot::channel();
        self.jobs
            .send(Job {
                snapshot,
                reply,
                publishing,
            })
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
    /// The tree the task's checkpoint resolves to, when this run's own save
    /// committed it; a turn's lives with its head in `turn`.
    pub(super) lineage: Option<crate::recovery::Lineage>,
}

impl<'a, 'run> BoundaryOwner<'a, 'run> {
    pub(super) fn new(run: &'a Run<'run>, workspace: Workspace<'a>) -> (Sink<'a>, Self) {
        let (send, jobs) = mpsc::channel(1);
        (
            Sink {
                jobs: send,
                publishing: Arc::new(Semaphore::new(1)),
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
                lineage: None,
            },
        )
    }

    pub(super) async fn beside(mut self, work: impl Future<Output = Ending> + Send) -> Ending {
        tokio::pin!(work);
        let ending = loop {
            tokio::select! {
                biased;
                ending = &mut work => break ending,
                job = self.jobs.recv() => {
                    let Some(job) = job else { break work.await };
                    // Erased, so the run's future names no save's type.
                    let served: std::pin::Pin<Box<dyn Future<Output = Option<Ending>> + Send + '_>> =
                        Box::pin(self.serve(job, &mut work));
                    if let Some(ending) = served.await {
                        break ending;
                    }
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

    /// Pins under the pause and answers the run, then publishes beside it;
    /// the run's ending, when it ended meanwhile. Dropping the reply's
    /// receiver never drops the save, and the run's end waits for the
    /// publication rather than cancelling it.
    async fn serve(
        &mut self,
        job: Job,
        work: &mut std::pin::Pin<&mut (impl Future<Output = Ending> + Send)>,
    ) -> Option<Ending> {
        let Job {
            snapshot,
            reply,
            publishing,
        } = job;
        let stop = self.stop.clone();
        // Poll work too: its stop grace can end while the pin waits.
        let (pinned, mut ended) = {
            let mut pin = Box::pin(self.pin(snapshot));
            tokio::select! {
                biased;
                pinned = &mut pin => (pinned, None),
                ending = work.as_mut() => {
                    stop.cancel();
                    (pin.await, Some(ending))
                }
            }
        };
        let publish = match pinned {
            Pinned::Answered(saved) => {
                let _gone = reply.send(saved);
                return ended;
            }
            Pinned::Publish(publish) => publish,
        };
        let (receipt, publication) = Publication::channel();
        let _gone = reply.send(Ok(RecoverySave::Publishing(publication)));
        let mut published = Box::pin(self.publish(publish));
        let published = if ended.is_some() {
            published.await
        } else {
            tokio::select! {
                biased;
                published = &mut published => published,
                ending = work.as_mut() => {
                    ended = Some(ending);
                    published.await
                }
            }
        };
        let _gone = receipt.send(published);
        drop(publishing);
        ended
    }
}

#[cfg(test)]
#[path = "recovery_boundary_tests.rs"]
mod tests;
