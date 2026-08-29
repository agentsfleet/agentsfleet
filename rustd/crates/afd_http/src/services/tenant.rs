//! The tenant plane's HTTP seams: ownership, the workspace directory, api-keys,
//! and terminal credentials.

use afd_auth::principal::Principal;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::paging::Page;
use afd_tenant::apikey::{ApiKeySort, Deactivation, Listing, MintRequest, Revealed, Revoked};
use afd_tenant::workspace::directory::{After, Created, WorkspacePage};
use afd_tenant::workspace::name::Chosen;
// Renamed at the import, not at the definition: `afd_tenant::apikey` already
// spells `MintRequest`, `Revealed` and `Revoked` for the api-key family, and
// this file names both families. The `Cli` prefix belongs to the collision, so
// it lives here rather than in the crate that has no collision.
use afd_tenant::cli_credential::{
    MintRequest as CliMintRequest, Revealed as CliRevealed, Revoked as CliRevoked, UserIdentity,
};

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
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send;

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
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        Self::authorize(self, principal, workspace)
    }

    fn tenant_of(
        &self,
        principal: &Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        Self::tenant_of(self, principal)
    }
}

/// The tenant's workspace directory — the list, and the create beside it.
///
/// Separate from [`WorkspaceOwnership`] though production answers both with
/// one value: the ownership seam is mounted as a LAYER and its suite stub
/// owns exactly one workspace to prove both halves of the refusal matrix,
/// while these verbs are ordinary handler calls whose stub refuses like every
/// other store. One trait would force one stub to be both things.
pub trait TenantWorkspaces: Send + Sync + std::fmt::Debug + 'static {
    /// One page of the tenant's workspaces, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon
    /// cannot read.
    fn page(
        &self,
        tenant: &Uuid7,
        filter: Option<&str>,
        after: Option<&After>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<WorkspacePage>> + Send;

    /// Creates one workspace, naming it when the caller did not.
    ///
    /// # Errors
    /// Refuses a session whose tenant has no row behind it and a chosen name
    /// this tenant already uses; reports a host that cannot draw entropy and
    /// a datastore that would not answer.
    fn create(
        &self,
        tenant: &Uuid7,
        chosen: Option<Chosen>,
        created_by: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Created>> + Send;
}

/// The production directory answers it directly.
impl TenantWorkspaces for afd_tenant::workspace::Workspaces {
    fn page(
        &self,
        tenant: &Uuid7,
        filter: Option<&str>,
        after: Option<&After>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<WorkspacePage>> + Send {
        Self::page(self, tenant, filter, after, limit)
    }

    fn create(
        &self,
        tenant: &Uuid7,
        chosen: Option<Chosen>,
        created_by: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Created>> + Send {
        Self::create(self, tenant, chosen, created_by, now)
    }
}

/// A person's command-line credentials — mint and revoke, and nothing else.
///
/// No list verb, deliberately: `cli_credentials.zig` serves `POST` and
/// `DELETE` only. Its module comment describes a `GET` that would show which
/// terminals hold a credential, and `route_table_invoke.zig` admits `POST`
/// alone — so the list is documented and not served. It is not ported here
/// because there is nothing to port; adding one would be a new endpoint in a
/// milestone whose rule is parity.
pub trait TerminalCredentials: Send + Sync + std::fmt::Debug + 'static {
    /// Resolves a proven subject to the user row these verbs write against.
    ///
    /// # Errors
    /// Refuses a subject with no user row. Reports a datastore that would not
    /// answer.
    fn user_of(
        &self,
        subject: &str,
    ) -> impl Future<Output = afd_tenant::Result<UserIdentity>> + Send;

    /// Mints this machine's credential, revoking whatever it left behind.
    ///
    /// # Errors
    /// Reports a host that cannot draw entropy and a datastore that would not
    /// answer.
    fn mint(
        &self,
        request: &CliMintRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<CliRevealed>> + Send;

    /// Revokes one of this user's credentials.
    ///
    /// # Errors
    /// Refuses an id naming no live credential this user holds.
    fn revoke(
        &self,
        user: &Uuid7,
        credential: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<CliRevoked>> + Send;
}

impl TerminalCredentials for afd_tenant::cli_credential::CliCredentials {
    fn user_of(
        &self,
        subject: &str,
    ) -> impl Future<Output = afd_tenant::Result<UserIdentity>> + Send {
        Self::user_of(self, subject)
    }

    fn mint(
        &self,
        request: &CliMintRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<CliRevealed>> + Send {
        Self::mint(self, request, now)
    }

    fn revoke(
        &self,
        user: &Uuid7,
        credential: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<CliRevoked>> + Send {
        Self::revoke(self, user, credential, now)
    }
}

/// Minting, listing, revoking and deleting a tenant's own credentials.
///
/// Every method takes ALREADY-PARSED values — a `KeyName` cannot hold a
/// space, a [`Deactivation`] cannot mean "make it live again" — so there is no
/// validation arm in any implementation, and none a stub could get differently
/// right from the real one.
pub trait TenantKeys: Send + Sync + std::fmt::Debug + 'static {
    /// Mints one key, answering the only view of its plaintext that exists.
    ///
    /// # Errors
    /// Refuses a name this tenant already uses; reports a host that cannot draw
    /// entropy and a datastore that would not answer.
    fn mint(
        &self,
        request: &MintRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Revealed>> + Send;

    /// One page of this tenant's keys, and its whole key count.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn list(
        &self,
        tenant: &Uuid7,
        page: &Page<ApiKeySort>,
    ) -> impl Future<Output = afd_tenant::Result<Listing>> + Send;

    /// Revokes one key, reporting only when this call did it.
    ///
    /// # Errors
    /// Refuses an id naming no key this tenant holds, and one already revoked.
    fn revoke(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
        intent: Deactivation,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Revoked>> + Send;

    /// Deletes one already-revoked key.
    ///
    /// # Errors
    /// Refuses an id naming no key this tenant holds, and one still active.
    fn delete(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send;
}

/// The production store answers it directly.
impl TenantKeys for afd_tenant::apikey::ApiKeys {
    fn mint(
        &self,
        request: &MintRequest<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Revealed>> + Send {
        Self::mint(self, request, now)
    }

    fn list(
        &self,
        tenant: &Uuid7,
        page: &Page<ApiKeySort>,
    ) -> impl Future<Output = afd_tenant::Result<Listing>> + Send {
        Self::list(self, tenant, page)
    }

    fn revoke(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
        intent: Deactivation,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Revoked>> + Send {
        Self::revoke(self, tenant, key, intent, now)
    }

    fn delete(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        Self::delete(self, tenant, key)
    }
}
