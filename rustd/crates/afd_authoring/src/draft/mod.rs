//! Creator-scoped draft lifecycle; every operation owns one database transaction.

mod create;
mod list;
mod patch;
pub(crate) use patch::idle;
pub(crate) mod read;

use crate::{Result, error::raise::stored};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::Analytics;

/// Native private setup service, shared by browser and terminal chat.
#[derive(Debug, Clone)]
pub struct Drafts {
    pub(crate) database: Db,
    pub(crate) entropy: Entropy,
    pub(crate) analytics: Analytics,
}

impl Drafts {
    /// Shares the daemon's pool; private drafts hold no provider keys.
    ///
    /// Reports no funnel step until [`Drafts::with_analytics`] names a reporter.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self {
            database,
            entropy,
            analytics: Analytics::silent(),
        }
    }

    /// Reports the private-chat-to-fleet funnel steps to `analytics`. Every
    /// service built over these drafts — setup, creations — shares it.
    #[must_use]
    pub fn with_analytics(self, analytics: Analytics) -> Self {
        Self { analytics, ..self }
    }

    pub(crate) fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }
}

pub(crate) fn etag(id: &Uuid7, version: i64) -> String {
    afd_core::etag::versioned(id, version)
}

pub(crate) fn value<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(stored)
}
