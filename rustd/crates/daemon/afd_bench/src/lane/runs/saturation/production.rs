//! One guarded native session per saturation host, sharing the daemon's domain handles.

use super::requester::{Assigned, Held, Host};
use super::stage::WORKERS;
use crate::lane::runs::{
    drain::{connection, native},
    seed::RunnerIdentity,
};
use crate::{
    datastores::Datastores,
    error::{ErrorKind, Result},
};
use af_core::id::Uuid7;
use af_crypto::secret::Kek;
use af_wire::{
    protocol::{Down, Renewal, Up},
    report::RenewRequest,
    run::{AcceptedKinds, Owner, TaggedRun},
    usage::RequestLines,
};
use afd_gateway::{Gateway, test_util::RunnerConnection};
use afd_setup::Drafts;
use std::{collections::BTreeMap, num::NonZeroU64, sync::Arc};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(super) struct Production {
    gateway: Gateway,
    pub(super) drafts: Drafts,
}

pub(super) struct Connected {
    hosts: BTreeMap<Uuid7, ConnectedHost>,
    stop: CancellationToken,
    listeners: Mutex<tokio::task::JoinSet<()>>,
}

struct ConnectedHost {
    session: Mutex<RunnerConnection>,
    generations: Mutex<watch::Receiver<afd_bus::wake::Generation>>,
}

impl Production {
    pub(super) fn new(stores: &Datastores, kek: &Arc<Kek>) -> Self {
        let (gateway, drafts) = connection::gateway(stores, kek);
        Self { gateway, drafts }
    }

    pub(super) async fn connect(
        &self,
        stores: &Datastores,
        runners: &[RunnerIdentity],
    ) -> Result<Connected> {
        let wake = afd_bus::wake::Wake::default();
        let stop = CancellationToken::new();
        let listeners = wake.listen(&stores.bus, &stop).await?;
        let mut hosts = BTreeMap::new();
        for runner in runners {
            let session = connection::open(
                &self.gateway,
                runner,
                AcceptedKinds::ALL,
                usize::try_from(WORKERS)?,
            )
            .await?;
            hosts.insert(
                runner.id.clone(),
                ConnectedHost {
                    session: Mutex::new(session),
                    generations: Mutex::new(wake.generations()),
                },
            );
        }
        Ok(Connected {
            hosts,
            stop,
            listeners: Mutex::new(listeners),
        })
    }
}

impl Connected {
    fn host(&self, runner: &Uuid7) -> Result<&ConnectedHost> {
        self.hosts.get(runner).ok_or_else(|| {
            ErrorKind::TaskLost {
                role: "native saturation host",
            }
            .into()
        })
    }

    pub(super) async fn close(&self) {
        self.stop.cancel();
        self.listeners.lock().await.shutdown().await;
        for host in self.hosts.values() {
            host.session.lock().await.shutdown().await;
        }
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

impl Host for Connected {
    async fn assign(&self, runner: &Uuid7) -> Result<Assigned> {
        let host = self.host(runner)?;
        let mut session = host.session.lock().await;
        let Some(frame) = session.assign().await? else {
            return Ok(Assigned::Nothing);
        };
        // A setup chat's turn rides the common run path as a run its draft owns.
        match native::decode(&frame)? {
            Down::Run {
                run: TaggedRun::Fleet(run),
            } => match run.owner {
                Some(Owner::Draft { .. }) => Ok(Assigned::Chat(Held {
                    run: Uuid7::parse(&run.run_id)?,
                    fence: run.fencing_token.to_string(),
                })),
                Some(Owner::Fleet { .. }) | None => Ok(Assigned::Fleet),
            },
            _ => Err(ErrorKind::StreamAnswer.into()),
        }
    }

    async fn wait(&self, runner: &Uuid7) -> Result<()> {
        self.host(runner)?
            .generations
            .lock()
            .await
            .changed()
            .await?;
        Ok(())
    }

    async fn renew(&self, runner: &Uuid7, held: &Held) -> Result<()> {
        let frame = self
            .host(runner)?
            .session
            .lock()
            .await
            .request(&Up::RenewBatch {
                request_id: NonZeroU64::MIN,
                runs: vec![Renewal {
                    run_id: held.run.as_str().into(),
                    request: RenewRequest {
                        requests: RequestLines(Vec::new()),
                    },
                }],
            })
            .await?;
        let Down::Renewed { runs, .. } = native::decode(&frame)? else {
            return Err(ErrorKind::StreamAnswer.into());
        };
        let [renewed] = <[_; 1]>::try_from(runs).map_err(|_count| ErrorKind::StreamAnswer)?;
        if renewed.run_id != held.run.as_str() {
            return Err(ErrorKind::StreamAnswer.into());
        }
        native::accepted(renewed.answer)?;
        Ok(())
    }
}
