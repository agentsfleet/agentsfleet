//! Creator-scoped draft lifecycle; every operation owns one database transaction.

mod create;
mod list;
mod patch;
pub(crate) use patch::idle;
pub(crate) mod read;

use std::sync::Arc;

use crate::{Result, error::raise::stored};
use afd_billing::biller::Biller;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::Analytics;
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
    /// (billing §11), built once where the daemon is assembled.
    pub(crate) biller: Arc<dyn Biller>,
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
        }
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
    afd_core::etag::versioned(id, version)
}

pub(crate) fn value<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(stored)
}

/// A stored vocabulary word read as the enum that spells it.
pub(crate) fn word<T: serde::de::DeserializeOwned>(word: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(word.to_owned())).map_err(stored)
}
