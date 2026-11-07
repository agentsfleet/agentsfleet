//! A signup writeback that RECORDS, beside a provider nobody can dial.
//!
//! # Why this one is a stub when [`super`] argues against them
//!
//! That header's rule is that a store inventing a refusal keeps agreeing with
//! the suite after the real store stops producing it, so every seam holds the
//! production store over a datastore that answers nothing. The rule holds and
//! this does not break it, because there is no dead pool to hold the real
//! writer over: `ProviderMetadata` refuses at a SOCKET, not at an acquire, and
//! a suite that stood one up would be proving reqwest's timeout.
//!
//! What is worth proving is the ordering the handler owns — that a bootstrapped
//! account's tenant is handed to the provider at all, under the subject the
//! event named, carrying the owner grant. That is unreachable without a seam
//! that says yes, and it is precisely the step whose absence shipped: the Rust
//! route created tenants and told the provider nothing for the whole of the
//! port.

use std::sync::{Arc, Mutex};

use afd_api::services::SignupMetadata;
use afd_auth::principal::Subject;
use afd_identity::MetadataUnwritten;

/// One writeback the handler asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WroteBack {
    /// The provider subject it addressed.
    pub(crate) subject: String,
    /// The tenant it carried.
    pub(crate) tenant_id: String,
    /// The claim it seeded.
    pub(crate) scopes: String,
}

/// A writeback seam that answers and remembers.
///
/// `Arc<Mutex<..>>` rather than a channel: a suite asserts AFTER the response,
/// when the call has already happened, so what it needs is a log to read rather
/// than a rendezvous to wait on.
#[derive(Debug, Clone, Default)]
pub(crate) struct RecordingWriteback {
    written: Arc<Mutex<Vec<WroteBack>>>,
    /// What the provider answers with, when a dimension needs it to refuse.
    outcome: Option<MetadataUnwritten>,
}

impl RecordingWriteback {
    /// A seam that accepts every write.
    pub(crate) fn accepting() -> Self {
        Self::default()
    }

    /// A seam that refuses every write, so a suite can prove the handler still
    /// answers the delivery.
    pub(crate) fn refusing(outcome: MetadataUnwritten) -> Self {
        Self {
            written: Arc::new(Mutex::new(Vec::new())),
            outcome: Some(outcome),
        }
    }

    /// Everything the handler wrote, in order.
    pub(crate) fn written(&self) -> Vec<WroteBack> {
        self.written
            .lock()
            .expect("the writeback log is healthy")
            .clone()
    }
}

impl SignupMetadata for RecordingWriteback {
    fn write_signup(
        &self,
        subject: &Subject,
        tenant_id: &str,
        scopes: &str,
    ) -> impl std::future::Future<Output = Result<(), MetadataUnwritten>> + Send {
        self.written
            .lock()
            .expect("the writeback log is healthy")
            .push(WroteBack {
                subject: subject.as_str().to_owned(),
                tenant_id: tenant_id.to_owned(),
                scopes: scopes.to_owned(),
            });
        // Ready rather than `async`: recording is synchronous, and an
        // `async` body with no await would be a future that only looks like it
        // does work.
        std::future::ready(self.outcome.map_or(Ok(()), Err))
    }
}
