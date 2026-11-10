//! One guarded native session per saturation host, sharing the daemon's domain handles.

use super::requester::{Held, Host, Offered};
use super::stage::WORKERS;
use crate::lane::lease::{
    drain::{native, session},
    seed::RunnerIdentity,
};
use crate::{
    datastores::Datastores,
    error::{ErrorKind, Result},
};
use af_core::id::Uuid7;
use af_crypto::secret::Kek;
use af_wire::{
    authoring::{Knowledge, Meter, Renew},
    lease::{AcceptedKinds, TaggedLease},
    protocol::{Down, Up},
};
use afd_authoring::Drafts;
use afd_gateway::{Gateway, test_util::Session};
use std::{collections::BTreeMap, num::NonZeroU64, sync::Arc};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

const IDLE: Meter = Meter {
    knowledge: Knowledge::Known,
    calls: 0,
    input_tokens: None,
    output_tokens: None,
    runtime_ms: 0,
    provider_cost_nanos: None,
};

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
    session: Mutex<Session>,
    generations: Mutex<watch::Receiver<u64>>,
}

impl Production {
    pub(super) fn new(stores: &Datastores, kek: &Arc<Kek>) -> Result<Self> {
        let (gateway, drafts) = session::gateway(stores, kek)?;
        Ok(Self { gateway, drafts })
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
            let session = session::open(
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
    async fn offer(&self, runner: &Uuid7) -> Result<Offered> {
        let host = self.host(runner)?;
        let mut session = host.session.lock().await;
        let Some(frame) = session.offer().await? else {
            return Ok(Offered::Nothing);
        };
        match native::decode(&frame)? {
            Down::Offer {
                lease: TaggedLease::Fleet(_),
            } => Ok(Offered::Fleet),
            Down::Offer {
                lease: TaggedLease::Authoring(lease),
            } => Ok(Offered::Chat(Held {
                lease: Uuid7::parse(&lease.lease_id)?,
                fence: lease.fence,
            })),
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
            .request(&Up::AuthoringRenew {
                request_id: NonZeroU64::MIN,
                lease_id: held.lease.as_str().into(),
                request: Renew {
                    fence: held.fence.clone(),
                    usage: IDLE,
                },
            })
            .await?;
        let Down::AuthoringRenewed {
            lease_id, answer, ..
        } = native::decode(&frame)?
        else {
            return Err(ErrorKind::StreamAnswer.into());
        };
        if lease_id != held.lease.as_str() {
            return Err(ErrorKind::StreamAnswer.into());
        }
        native::accepted(answer)?;
        Ok(())
    }
}
