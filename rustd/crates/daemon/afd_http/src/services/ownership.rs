//! Deciding whose workspace a request is acting in: the one tenant seam the
//! shared ownership guard asks, so every plane reaches it.

use af_core::id::Uuid7;
use afd_auth::principal::Principal;
use afd_tenant::workspace::access::Access;

/// Deciding whose workspace a request is acting in.
///
/// One method, because ownership is one question. It is a TRAIT rather than a
/// concrete call for the reason every other seam here is: the router suites
/// prove the refusal matrix in front of the handlers, and a matrix that needed
/// a live Postgres to prove would not be proven.
pub trait WorkspaceOwnership: Send + Sync + std::fmt::Debug + 'static {
    /// The tenant owning `workspace`, when this principal's tenant does.
    ///
    /// `Ok(None)` is a DENIAL and `Err` is an outage, and the two must never
    /// collapse: answering "not yours" for a pool timeout would tell a tenant
    /// their own workspace had vanished (RULE ECL).
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn authorize(
        &self,
        principal: &Principal,
        workspace: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Option<Access>>> + Send;

    /// The tenant a principal resolves to with no workspace to check against.
    ///
    /// The cold path the tenant plane's own routes take: `/v1/api-keys` acts on
    /// whatever the credential resolved to, so there is no identifier to
    /// authorize and this is what says which rows are in scope.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn tenant_of(
        &self,
        principal: &Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send;
}

/// The production resolver answers it directly.
impl WorkspaceOwnership for afd_tenant::workspace::Workspaces {
    fn authorize(
        &self,
        principal: &Principal,
        workspace: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Option<Access>>> + Send {
        Self::authorize(self, principal, workspace)
    }

    fn tenant_of(
        &self,
        principal: &Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        Self::tenant_of(self, principal)
    }
}
