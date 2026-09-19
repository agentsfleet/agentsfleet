//! The HTTP seam the caller-identity read acts through.
//!
//! Its own file rather than a third trait in [`super::tenant`], for the reason
//! the preference and provider seams have their own: the file next door already
//! carries ownership, the workspace directory, api-keys and terminal
//! credentials, and a fifth concern there buys nothing but a longer file.
//!
//! # Why the subject is resolved here and not before the handler runs
//!
//! [`super::preference`] records the same answer at length. The principal a
//! guard builds carries the identity provider's SUBJECT; this read answers with
//! the `core.users` row that subject maps to, and the mapping is a statement
//! like any other. The difference from that seam is the shape of "no row": a
//! preference read answers `Ok(None)` and lets its handler refuse, while this
//! one refuses in the store, because a caller-identity read has nothing to
//! answer without one and the refusal sentence is already registered.

use afd_tenant::Result as TenantResult;
use afd_tenant::identity::Profile;

/// Resolving a proven subject to the person it names.
pub trait CallerProfiles: Send + Sync + std::fmt::Debug + 'static {
    /// The person this identity-provider subject belongs to.
    ///
    /// # Errors
    /// Refuses a subject with no `core.users` row — a credential that
    /// authenticates and names nobody here. Reports a datastore that would not
    /// answer, and a stored identifier that is not a version-7 Universally
    /// Unique Identifier (UUID).
    fn profile(&self, subject: &str) -> impl Future<Output = TenantResult<Profile>> + Send;
}

/// The production directory answers it directly.
impl CallerProfiles for afd_tenant::identity::Identities {
    fn profile(&self, subject: &str) -> impl Future<Output = TenantResult<Profile>> + Send {
        Self::profile(self, subject)
    }
}
