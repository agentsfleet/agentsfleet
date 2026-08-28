//! The seam the workspace secret surface acts through.
//!
//! One trait over all four verbs, because they are one store and a suite that
//! stubbed them separately would be stubbing an implementation detail. Every
//! method takes ALREADY-PARSED values: a [`SecretName`] cannot be empty or
//! over-long, and a [`SecretBody`] cannot be anything but a non-empty JSON
//! object within its bound. So there is no validation arm in any
//! implementation, and none a stub could get differently right from the real
//! one.
//!
//! # The store is split behind this trait, and the split is the guarantee
//!
//! `afd_vault::Vault` seals; `afd_vault::Directory` holds no key and cannot
//! decrypt. The list and the delete are answered by the second, which is why
//! "the list performs zero decrypts" survives a future edit to the store — a
//! decrypt added to the list path would not compile, rather than passing review.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_vault::{Deleted, Result as VaultResult, SecretBody, SecretName, SecretSummary};

/// Everything the workspace secret routes act through.
///
/// A trait rather than the concrete store for the reason every seam in this
/// module is one: the router suites prove the refusal matrix in FRONT of the
/// verbs, and a matrix that needed a live Postgres to prove would not be proven.
pub trait WorkspaceSecrets: Send + Sync + std::fmt::Debug + 'static {
    /// Seals `body` under a name this workspace does not yet hold.
    ///
    /// # Errors
    /// Refuses a name the workspace already holds, and nothing is written.
    /// Reports a datastore that would not answer and an envelope that would not
    /// seal.
    fn store(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> impl Future<Output = VaultResult<()>> + Send;

    /// Every secret this workspace holds, as its non-secret descriptors.
    ///
    /// Performs no decrypt — see the module note.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A row this build cannot
    /// LABEL is not an error; it lists as an opaque credential.
    fn list(
        &self,
        workspace: &Uuid7,
    ) -> impl Future<Output = VaultResult<Vec<SecretSummary>>> + Send;

    /// Replaces the whole body of a secret this workspace already holds.
    ///
    /// # Errors
    /// Refuses a name this workspace does not hold, creating nothing. Reports a
    /// datastore that would not answer and an envelope that would not seal.
    fn replace(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> impl Future<Output = VaultResult<()>> + Send;

    /// Removes one secret, under the model-registry reference lock.
    ///
    /// # Errors
    /// Refuses a credential the tenant's model registry still names, reporting
    /// how many entries did. Reports a datastore that would not answer. A name
    /// nothing is held under is NOT an error — the request got what it wanted.
    fn remove(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
    ) -> impl Future<Output = VaultResult<Deleted>> + Send;
}

/// The production store answers it through both of its halves.
impl WorkspaceSecrets for afd_vault::Vault {
    fn store(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> impl Future<Output = VaultResult<()>> + Send {
        Self::create(self, workspace, name, body, now)
    }

    fn list(
        &self,
        workspace: &Uuid7,
    ) -> impl Future<Output = VaultResult<Vec<SecretSummary>>> + Send {
        // Through the key-less half deliberately. The sealing value is right
        // here and would answer just as well; going through `directory()` is
        // what makes the never-decrypt guarantee visible at the call site
        // rather than only in the store.
        self.directory().list(workspace)
    }

    fn replace(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> impl Future<Output = VaultResult<()>> + Send {
        Self::replace(self, workspace, name, body, now)
    }

    fn remove(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
    ) -> impl Future<Output = VaultResult<Deleted>> + Send {
        self.directory().delete(workspace, name)
    }
}
