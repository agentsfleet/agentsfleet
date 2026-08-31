//! The HTTP seam the tenant model registry acts through.
//!
//! # A second trait over the same store, not a second store
//!
//! [`Providers`] answers both this and [`TenantProviders`](super::TenantProviders):
//! the registry's `active` flag is computed against the selection that trait
//! reads, and the page carries the platform default it reads. One store, and
//! [`Services::TenantProviders`](super::Services::TenantProviders) is bound by
//! both traits rather than gaining a second associated type and a second
//! accessor — a handler that has one has both.
//!
//! Two narrow traits rather than one wide one is `M-DI-HIERARCHY`'s own shape:
//! each names a surface, and a single spelling is reached by bounding on both
//! rather than by widening either. It also keeps the suites honest — a router
//! test proving the registry's refusal matrix stubs four verbs, not eight.
//!
//! # Every method takes ALREADY-PARSED values
//!
//! A limit is a `u32` the paging layer bounded, a boundary is a decoded
//! [`Boundary`] and not a token, an id is a [`Uuid7`] and not text. So there is
//! no validation arm in any implementation, and none a stub could get
//! differently right from the real one.
//!
//! # Why the cursor does not cross this seam
//!
//! A registry cursor binds the walk to the tenant and page size that issued it,
//! and refuses a token issued under either. Only the handler knows the
//! authenticated tenant and the requested limit, so only the handler can make
//! that comparison — the store is handed the boundary it already trusts.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::Result as CredentialResult;
use afd_credential::provider::{Added, Boundary, Providers, RegistryPage, Removed, Retargeted};

/// Everything the tenant model registry routes act through.
pub trait TenantModelEntries: Send + Sync + std::fmt::Debug + 'static {
    /// One page of `tenant`'s registry, newest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read. A credential deleted out of band is not a failure — its row
    /// lists degraded, because a page of twenty models must not fail over one
    /// dangling reference.
    fn registry_page(
        &self,
        tenant: &Uuid7,
        limit: u32,
        after: Option<&Boundary>,
    ) -> impl Future<Output = CredentialResult<RegistryPage>> + Send;

    /// Registers `model_id` on the stored credential `secret_ref`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a host that cannot draw
    /// the entropy an id is minted from. Every refusal a client can provoke is
    /// an [`Added`] variant instead.
    fn add_entry(
        &self,
        tenant: &Uuid7,
        model_id: &str,
        secret_ref: &str,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<Added>> + Send;

    /// Points an entry at a different model, keeping its credential.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read. Both refusals are [`Retargeted`] variants.
    fn set_entry_model(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
        model_id: &str,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<Retargeted>> + Send;

    /// Removes an entry, unless it is what the tenant is running on.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read. Both outcomes are [`Removed`] variants — an id that does
    /// not resolve is [`Removed::Done`], because the caller wanted the row gone
    /// and it is.
    fn remove_entry(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Removed>> + Send;
}

/// The production store answers it directly.
impl TenantModelEntries for Providers {
    fn registry_page(
        &self,
        tenant: &Uuid7,
        limit: u32,
        after: Option<&Boundary>,
    ) -> impl Future<Output = CredentialResult<RegistryPage>> + Send {
        Self::registry_page(self, tenant, limit, after)
    }

    fn add_entry(
        &self,
        tenant: &Uuid7,
        model_id: &str,
        secret_ref: &str,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<Added>> + Send {
        Self::add_entry(self, tenant, model_id, secret_ref, now)
    }

    fn set_entry_model(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
        model_id: &str,
        now: UnixMillis,
    ) -> impl Future<Output = CredentialResult<Retargeted>> + Send {
        Self::set_entry_model(self, tenant, entry_id, model_id, now)
    }

    fn remove_entry(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
    ) -> impl Future<Output = CredentialResult<Removed>> + Send {
        Self::remove_entry(self, tenant, entry_id)
    }
}
