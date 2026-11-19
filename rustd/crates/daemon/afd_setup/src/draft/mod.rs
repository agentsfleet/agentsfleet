//! Creator-scoped draft lifecycle; every operation owns one database transaction.

mod create;
mod list;
mod patch;
pub(crate) use patch::idle;
pub(crate) mod read;

use std::sync::Arc;

use crate::{Result, error::raise::stored};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_observability::Analytics;
use afd_billing::biller::Biller;
use afd_db::Db;
use moka::future::Cache;
use tokio::sync::Mutex;

/// Eviction may split a local queue; the database lock still orders every write.
const MAX_CACHED_SEND_GATES: u64 = 1024;

/// Native private setup service, shared by browser and terminal chat.
#[derive(Debug, Clone)]
pub struct Drafts {
    pub(crate) turn_policy: crate::turn_policy::TurnPolicy,
    pub(crate) clock: af_core::timing::LeaseClock,
    pub(crate) database: Db,
    pub(crate) entropy: Entropy,
    pub(crate) analytics: Analytics,
    /// Clones share these content-free queues, so hot chats wait outside the
    /// pool. They cache neither authorization nor a database ordering decision.
    pub(crate) senders: Cache<Uuid7, Arc<Mutex<()>>>,
    /// Moves a setup chat's money: the same biller fleet leases bill through
    /// (one billing lifecycle behind the `Biller` trait, per
    /// `docs/architecture/billing_and_provider_keys.md`), built once where the
    /// daemon is assembled.
    pub(crate) biller: Arc<dyn Biller>,
    pub(crate) authority: Option<afd_runner::capacity::authority::Authority>,
    /// Carries the hints a draft lease needs after a commit: the wake that
    /// offers an accepted message, the Cancel a Stop or a deletion sends, and
    /// a setup lease's live answer. `None` publishes nothing; the 5 s probe
    /// still finds the work.
    pub(crate) bus: Option<afd_bus::Handle>,
}

impl Drafts {
    /// Applies checked limits to newly admitted attempts; saved attempts retain their own policy.
    #[must_use]
    pub fn with_turn_policy(mut self, policy: crate::turn_policy::TurnPolicy) -> Self {
        self.turn_policy = policy;
        self
    }

    /// Uses the deployment clock validated before startup.
    #[must_use]
    pub fn with_clock(mut self, clock: af_core::timing::LeaseClock) -> Self {
        self.clock = clock;
        self
    }

    /// Shares the daemon's pool and its `biller`; private drafts hold no
    /// provider keys.
    ///
    /// Reports no funnel step until [`Drafts::with_analytics`] names a reporter.
    #[must_use]
    pub fn new(database: Db, entropy: Entropy, biller: Arc<dyn Biller>) -> Self {
        Self {
            clock: af_core::timing::LeaseClock::default(),
            turn_policy: crate::turn_policy::TurnPolicy::default(),
            database,
            entropy,
            analytics: Analytics::silent(),
            senders: Cache::new(MAX_CACHED_SEND_GATES),
            biller,
            authority: None,
            bus: None,
        }
    }

    /// Publishes this service's wake, Cancel and live-answer hints on `bus`.
    #[must_use]
    pub fn with_bus(mut self, bus: afd_bus::Handle) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Hints, after the commit that queued a message on `draft`, that a draft
    /// lease may take it. One wake key names the owner, fleet or draft, by its
    /// slot's id. Lossy: a lost hint costs one 5 s probe.
    pub(crate) async fn wake_setup(&self, draft: &Uuid7) {
        if let Some(bus) = &self.bus {
            afd_bus::wake::publish(bus, draft.clone()).await;
        }
    }

    /// Sends Cancel to each runner whose draft lease a commit just killed or
    /// deleted. A hint: the durable mark or the missing row refuses the
    /// runner's next renewal either way.
    pub(crate) async fn announce_cancelled(&self, cancelled: Vec<(Uuid7, Uuid7)>) {
        let Some(bus) = &self.bus else {
            return;
        };
        for (runner, lease_id) in cancelled {
            let subject = afd_bus::Subject::RunnerCommand(runner.clone());
            let publication = afd_bus::Command::Cancel { lease_id }
                .publish(bus, &runner)
                .await;
            afd_bus::observe_notification(&subject, publication);
        }
    }

    /// Fence streamed authoring grants with the same owner as fleet grants.
    #[must_use]
    pub fn for_connection(mut self, authority: afd_runner::capacity::authority::Authority) -> Self {
        self.authority = Some(authority);
        self
    }

    /// Lock the runner row through this transaction and count its occupied workers.
    /// Stream grants also recheck the opening credential and connection owner.
    pub(crate) async fn lock_host_capacity(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner: &Uuid7,
        claimant: afd_runner::capacity::Claimant<'_>,
        work_tenant: Option<&Uuid7>,
    ) -> Result<Option<afd_runner::capacity::Capacity>> {
        Ok(match &self.authority {
            Some(authority) => {
                afd_runner::capacity::lock(tx, (runner, authority), claimant, work_tenant).await?
            }
            None => afd_runner::capacity::lock(tx, runner, claimant, work_tenant).await?,
        })
    }

    /// Recheck a streamed mutation after its draft/attempt lock, preserving lock order.
    pub(crate) async fn check_connection(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner: &Uuid7,
    ) -> Result<()> {
        if self.authority.is_some() {
            let host = self
                .lock_host_capacity(tx, runner, afd_runner::capacity::Claimant::Existing, None)
                .await?
                .ok_or(crate::error::ErrorKind::Hidden)?;
            if !host.can_renew() {
                return Err(crate::error::ErrorKind::Fence.into());
            }
        }
        Ok(())
    }

    /// Reports the private-chat-to-fleet funnel steps to `analytics`. Every
    /// service built over these drafts — setup, creations — shares it.
    #[must_use]
    pub fn with_analytics(self, analytics: Analytics) -> Self {
        Self { analytics, ..self }
    }

    pub(crate) fn mint(&self, now: jiff::Timestamp) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }
}

pub(crate) fn etag(id: &Uuid7, version: i64) -> String {
    af_core::etag::versioned(id, version)
}

pub(crate) fn value<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(stored)
}

/// A stored vocabulary word read as the enum that spells it.
pub(crate) fn word<T: serde::de::DeserializeOwned>(word: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(word.to_owned())).map_err(stored)
}
