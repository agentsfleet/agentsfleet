//! Reviewed native setup uses the same creator guard and encrypted credential store.

use std::sync::Arc;

use afd_credential::{secrets::Registry, vault::Vault};
use afd_crypto::secret::Kek;

use crate::Drafts;

/// Human-reviewed setup and Library operations over native immutable candidates.
#[derive(Debug, Clone)]
pub struct Setup {
    pub(crate) drafts: Drafts,
    pub(crate) vault: Vault,
    pub(crate) connectors: Registry,
}

impl Setup {
    /// Shares the daemon's pool and process key; no provider call runs on review.
    #[must_use]
    pub fn new(drafts: Drafts, key: Arc<Kek>) -> Self {
        Self {
            vault: Vault::new(drafts.database.clone(), key),
            connectors: Registry::SHIPPED,
            drafts,
        }
    }
}
