//! Where the scheduler client reads its credentials from, and what a read that
//! fails leaves in the log.

use af_core::id::Uuid7;
use afd_vault::Vault;

use crate::credentials::{Credentials, QSTASH_BAG};
use crate::error::{self, Result};

/// The scheduler credentials could not be read, so no call went out.
const EVENT_BAG_UNUSABLE: &str = "scheduler_bag_unusable";

/// Where [`super::QStash`] reads its credentials.
#[derive(Debug, Clone)]
pub(super) enum Source {
    /// The admin workspace's vault: this deployment's own `qstash` bag.
    Vault { vault: Vault, admin: Option<Uuid7> },
    /// Credentials a suite fixed, for the suites that drive a stub scheduler.
    #[cfg(any(test, feature = "test-util"))]
    Fixed(Credentials),
}

impl Source {
    /// The credentials as the `qstash` bag holds them now.
    ///
    /// A fault is logged here, once per read, with the field it names and never
    /// a value. The fire route logs the refusal it answers under its own event,
    /// so a rejected fire shows both.
    ///
    /// # Errors
    /// An unusable bag (absent, malformed, a field missing) and a vault that
    /// would not answer, the second retryable.
    pub(super) async fn read(&self) -> Result<Credentials> {
        let read = self.fetch().await;
        if let Err(error) = &read {
            log_unusable(error);
        }
        read
    }

    async fn fetch(&self) -> Result<Credentials> {
        match self {
            Self::Vault { vault, admin } => vault
                .bag::<Credentials>(admin.as_ref(), QSTASH_BAG)
                .await?
                .map_err(error::bag_unusable),
            #[cfg(any(test, feature = "test-util"))]
            Self::Fixed(credentials) => Ok(credentials.clone()),
        }
    }
}

/// Logs a credentials read that failed: the fault's name and field, or the
/// vault's code, never a value.
fn log_unusable(error: &crate::Error) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let error_code = error.code().as_str();
    if let Some(fault) = error.bag_fault() {
        fault.warn(EVENT_BAG_UNUSABLE, QSTASH_BAG, error_code);
    } else {
        let reason = error.refusal_word();
        tracing::warn!(event = EVENT_BAG_UNUSABLE, error_code, reason);
    }
}
