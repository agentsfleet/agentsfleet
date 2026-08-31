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
//! # Why there is no credential-probe verb here
//!
//! The write ladder's credential checks run under the reference lock, inside
//! ONE transaction — a seam of separate pool-acquiring verbs cannot form one.
//! Activation is therefore a single store verb (landing with the PUT handler),
//! and this seam carries only the verbs whose statements stand alone.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::Result as CredentialResult;
use afd_credential::provider::{PlatformDefault, Providers, Selection};

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

    fn upsert(
        &self,
        tenant: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<()>> + Send {
        Self::upsert(self, tenant, selection, now)
    }
}
