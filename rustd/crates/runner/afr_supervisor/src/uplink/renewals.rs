//! One host tick snapshots live meters without retaining map guards across work.

use std::sync::Arc;

use af_core::{error_code::ErrorCode, id::Uuid7, timing::RENEWAL_TICK};
use af_wire::{
    protocol::{LEASES_MAX, Renewal},
    report::RenewResponse,
};
use afr_agent::Meter;
use dashmap::{DashMap, mapref::entry::Entry};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    time::{Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

use super::StreamHandle;
use crate::{
    Result,
    client::stream::{Outcome, RenewalBatch},
    error::ErrorKind,
};

#[derive(Clone, Debug)]
pub(crate) enum Renewed {
    Confirmed(RenewResponse),
    Refused(ErrorCode),
}

struct Lease {
    meter: Arc<Meter>,
    updates: watch::Sender<Option<Renewed>>,
}

pub(crate) struct Renewals {
    leases: DashMap<Uuid7, Arc<Lease>>,
    slots: Arc<Semaphore>,
}

impl Default for Renewals {
    fn default() -> Self {
        Self {
            leases: DashMap::new(),
            slots: Arc::new(Semaphore::new(LEASES_MAX)),
        }
    }
}

impl std::fmt::Debug for Renewals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renewals")
            .field("leases", &self.leases.len())
            .finish()
    }
}

impl Renewals {
    pub(crate) fn register(
        self: &Arc<Self>,
        lease: &Uuid7,
        meter: Arc<Meter>,
    ) -> Result<Registration> {
        let slot = Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(|_| ErrorKind::StreamProtocol)?;
        let (updates, watching) = watch::channel(None);
        match self.leases.entry(lease.clone()) {
            Entry::Occupied(_) => return Err(ErrorKind::StreamProtocol.into()),
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(Lease { meter, updates }));
            }
        }
        Ok(Registration {
            registry: Arc::clone(self),
            lease: lease.clone(),
            updates: watching,
            _slot: slot,
        })
    }

    pub(crate) async fn run(&self, handle: &StreamHandle, stop: CancellationToken) -> Result<()> {
        let mut ticks = tokio::time::interval_at(Instant::now() + RENEWAL_TICK, RENEWAL_TICK);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! { () = stop.cancelled() => return Ok(()), _ = ticks.tick() => {} }
            let renewed = tokio::select! {
                () = stop.cancelled() => return Ok(()),
                result = self.batch(handle) => result,
            };
            match renewed {
                Ok(()) => {}
                Err(error) if error.is_retryable() => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "renew_failed_retry", error_code);
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn batch(&self, handle: &StreamHandle) -> Result<()> {
        let (ids, leases) = self.snapshot();
        if leases.is_empty() {
            return Ok(());
        }
        self.record(handle.compute(RenewalBatch { ids, leases }).await?);
        Ok(())
    }

    fn snapshot(&self) -> (Vec<Uuid7>, Vec<Renewal<'static>>) {
        // Clone identities and handles first. Meter locks never nest under a map shard.
        let leases: Vec<_> = self
            .leases
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        leases
            .into_iter()
            .map(|(id, lease)| {
                let request = af_wire::report::RenewRequest {
                    requests: lease.meter.unrecorded(),
                };
                let renewal = Renewal {
                    lease_id: id.to_string().into(),
                    request,
                };
                (id, renewal)
            })
            .unzip()
    }

    fn record(&self, leases: Vec<(Uuid7, Outcome<RenewResponse>)>) {
        for (id, answer) in leases {
            let lease = self.leases.get(&id).map(|entry| Arc::clone(entry.value()));
            let Some(lease) = lease else {
                continue;
            };
            // Releasing the shard before watch notification permits concurrent retirements.
            match answer {
                Outcome::Accepted(response) => {
                    lease
                        .updates
                        .send_replace(Some(Renewed::Confirmed(response)));
                }
                Outcome::Refused {
                    code,
                    retryable: false,
                } => {
                    lease.updates.send_replace(Some(Renewed::Refused(code)));
                }
                Outcome::Refused {
                    code,
                    retryable: true,
                } => {
                    let error_code = code.as_str();
                    tracing::debug!(event = "runner_stream_renewal_retry", error_code);
                }
            }
        }
    }
}

pub(crate) struct Registration {
    registry: Arc<Renewals>,
    lease: Uuid7,
    pub(crate) updates: watch::Receiver<Option<Renewed>>,
    _slot: OwnedSemaphorePermit,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.registry.leases.remove(&self.lease);
    }
}
