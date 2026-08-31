//! The HTTP seam the tenant's own provider surface acts through.
//!
//! One trait over the whole store, because they are one store and a suite that
//! stubbed the read separately from the write would be stubbing an
//! implementation detail. Every method takes ALREADY-PARSED values — a
//! [`Selection`] holds a [`Posture`](afd_billing::Posture) and not a `mode`
//! string, a workspace is a [`Uuid7`] and not text — so there is no validation
//! arm in any implementation, and none a stub could get differently right from
//! the real one.
//!
//! # Why the reads are separate methods rather than one view
//!
//! The Zig answers `GET /v1/tenants/me/provider` with one statement and then
//! acquires a SECOND connection to ask whether a platform default exists,
//! because its simple-protocol connection cannot start a query while the first
//! result set is open. That is a driver constraint, not a shape the surface
//! wants: the two facts are independent — a tenant's own selection, and whether
//! the deployment has a default to fall back to — and the handler composes
//! them. Keeping them separate here means the composition is visible where it
//! is decided, and a suite can pin either half without the other.
//!
//! # Why the credential probe reads metadata and never a key
//!
//! [`TenantProviders::secret_kind`] answers what KIND of credential a name
//! holds off the vault's non-secret columns. The write path's refusals are the
//! most-walked path on this surface — a client naming a credential it has not
//! stored — and answering them without a decrypt means a plaintext key never
//! enters the process on the way to a 400.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::Result as CredentialResult;
use afd_credential::provider::{PlatformDefault, Providers, SecretKind, Selection};

/// Everything the tenant provider routes act through.
///
/// A trait rather than the concrete store for the reason every seam in this
/// module is one: the router suites prove the refusal matrix in FRONT of the
/// verbs, and a matrix that needed a live Postgres to prove would not be
/// proven.
pub trait TenantProviders: Send + Sync + std::fmt::Debug + 'static {
    /// What this tenant configured for itself, or nothing if it never has.
    ///
    /// `Ok(None)` is not a failure and not a default. It is the tenant who has
    /// never configured a provider, and the surface renders it differently from
    /// an explicit platform row — that distinction is the only reason the write
    /// path stores an explicit row at all.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored row this daemon
    /// cannot read.
    fn selection(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Option<Selection>>> + Send;

    /// The deployment's active platform default, or nothing if none is set.
    ///
    /// Read on this surface for two independent reasons: it fills the view of a
    /// tenant that has no selection of its own, and its mere PRESENCE is what
    /// lets the Models page gate a "switch to default" action before the click
    /// rather than after a failed write.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an active row carrying no
    /// model to price against.
    fn platform_default(
        &self,
    ) -> impl Future<Output = CredentialResult<Option<PlatformDefault>>> + Send;

    /// The workspace this tenant's self-managed credentials are held in.
    ///
    /// `Ok(None)` is a tenant with no workspace at all — a violated bootstrap
    /// invariant, since signup creates the primary workspace, and one no retry
    /// repairs.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an identifier column this
    /// daemon cannot read.
    fn primary_workspace(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Option<Uuid7>>> + Send;

    /// What kind of credential `workspace` holds under `name`.
    ///
    /// Decides both of the write ladder's credential rungs in one round trip
    /// and decrypts nothing — see the module note.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn secret_kind(
        &self,
        workspace: &Uuid7,
        name: &str,
    ) -> impl Future<Output = CredentialResult<SecretKind>> + Send;

    /// Writes this tenant's selection, last-write-wins on its single row.
    ///
    /// Takes the coherent pair it is given. Platform mode naming a credential,
    /// and self-managed mode naming none, are refused where a request BECOMES a
    /// selection — so there is one place that says what a coherent selection
    /// is, and it is the one that can answer a client with a sentence.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a context ceiling wider
    /// than the column holds.
    fn upsert(
        &self,
        tenant: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<()>> + Send;
}

/// The production store answers it directly.
impl TenantProviders for Providers {
    fn selection(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Option<Selection>>> + Send {
        Self::selection(self, tenant)
    }

    fn platform_default(
        &self,
    ) -> impl Future<Output = CredentialResult<Option<PlatformDefault>>> + Send {
        Self::platform_default(self)
    }

    fn primary_workspace(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Option<Uuid7>>> + Send {
        Self::primary_workspace(self, tenant)
    }

    fn secret_kind(
        &self,
        workspace: &Uuid7,
        name: &str,
    ) -> impl Future<Output = CredentialResult<SecretKind>> + Send {
        Self::secret_kind(self, workspace, name)
    }

    fn upsert(
        &self,
        tenant: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<()>> + Send {
        Self::upsert(self, tenant, selection, now)
    }
}
