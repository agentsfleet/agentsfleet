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
}

impl Drafts {
    /// Shares the daemon's pool and its `biller`; private drafts hold no
    /// provider keys.
    ///
    /// Reports no funnel step until [`Drafts::with_analytics`] names a reporter.
    #[must_use]
    pub fn new(database: Db, entropy: Entropy, biller: Arc<dyn Biller>) -> Self {
        Self {
            database,
            entropy,
            analytics: Analytics::silent(),
            senders: Cache::new(MAX_CACHED_SEND_GATES),
            biller,
            authority: None,
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
    ) -> Result<Option<afd_runner::capacity::Capacity>> {
        Ok(match &self.authority {
            Some(authority) => afd_runner::capacity::lock(tx, (runner, authority)).await?,
            None => afd_runner::capacity::lock(tx, runner).await?,
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
                .lock_host_capacity(tx, runner)
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
