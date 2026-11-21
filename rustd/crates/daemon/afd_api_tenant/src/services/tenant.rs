//! The tenant's own HTTP seams: the workspace directory, api-keys and terminal
//! credentials.

use af_core::id::Uuid7;
use af_core::paging::Page;
use afd_auth::principal::Person;
use afd_tenant::apikey::{ApiKeySort, Deactivation, Listing, MintRequest, Revealed, Revoked};
use afd_tenant::workspace::accounts::Accounts;
use afd_tenant::workspace::directory::{After, Created, WorkspacePage};
use afd_tenant::workspace::name::Chosen;
use jiff::Timestamp;
// Renamed at the import, not at the definition: `afd_tenant::apikey` already
// spells `MintRequest`, `Revealed` and `Revoked` for the api-key family, and
// this file names both families. The `Cli` prefix belongs to the collision, so
// it lives here rather than in the crate that has no collision.
use afd_tenant::cli_credential::{
    MintRequest as CliMintRequest, Revealed as CliRevealed, Revoked as CliRevoked, UserIdentity,
};

/// The tenant's workspace directory — the list, and the create beside it.
///
/// Separate from [`afd_http::services::WorkspaceOwnership`] though production answers both with
/// one value: the ownership seam is mounted as a LAYER and its suite stub
/// owns exactly one workspace to prove both halves of the refusal matrix,
/// while these verbs are ordinary handler calls whose stub refuses like every
/// other store. One trait would force one stub to be both things.
pub trait TenantWorkspaces: Send + Sync + std::fmt::Debug + 'static {
    /// The accounts a person holds, with their role in each.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored value this
    /// daemon cannot read.
    fn accounts_of(
        &self,
        person: &Person,
    ) -> impl Future<Output = afd_tenant::Result<Accounts>> + Send;

    /// One page of the workspaces across `tenants`, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon
    /// cannot read.
    fn page(
        &self,
        tenants: &[&Uuid7],
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
        now: Timestamp,
    ) -> impl Future<Output = afd_tenant::Result<Created>> + Send;
}

/// The production directory answers it directly.
impl TenantWorkspaces for afd_tenant::workspace::Workspaces {
    fn accounts_of(
        &self,
        person: &Person,
    ) -> impl Future<Output = afd_tenant::Result<Accounts>> + Send {
        Self::accounts_of(self, person)
    }

    fn page(
        &self,
        tenants: &[&Uuid7],
        filter: Option<&str>,
        after: Option<&After>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<WorkspacePage>> + Send {
        Self::page(self, tenants, filter, after, limit)
    }

    fn create(
        &self,
        tenant: &Uuid7,
        chosen: Option<Chosen>,
        now: Timestamp,
    ) -> impl Future<Output = afd_tenant::Result<Created>> + Send {
        Self::create(self, tenant, chosen, now)
    }
}

/// A person's command-line credentials — mint and revoke, and nothing else.
///
/// No list verb, deliberately: `TenantRoute::CliCredentials` admits `POST` and
/// `TenantRoute::CliCredential` admits `DELETE`, and nothing else. A `GET` that
/// showed which terminals hold a credential would be a new endpoint, not a verb
/// this trait is missing.
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
        now: Timestamp,
    ) -> impl Future<Output = afd_tenant::Result<CliRevealed>> + Send;

    /// Revokes one of this user's credentials.
    ///
    /// # Errors
    /// Refuses an id naming no live credential this user holds.
    fn revoke(
        &self,
        user: &Uuid7,
        credential: &Uuid7,
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
        now: Timestamp,
    ) -> impl Future<Output = afd_tenant::Result<CliRevealed>> + Send {
        Self::mint(self, request, now)
    }

    fn revoke(
        &self,
        user: &Uuid7,
        credential: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<CliRevoked>> + Send {
        Self::revoke(self, user, credential)
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
        now: Timestamp,
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
        now: Timestamp,
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
    ) -> impl Future<Output = afd_tenant::Result<Revoked>> + Send {
        Self::revoke(self, tenant, key, intent)
    }

    fn delete(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        Self::delete(self, tenant, key)
    }
}
