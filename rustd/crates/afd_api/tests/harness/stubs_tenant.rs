//! The tenant plane's stubs: every seam a suite refuses through.
//!
//! One file for the six of them because they are one idea repeated — the
//! honest refusal of a store whose whole behaviour lives in a statement a
//! real datastore evaluates — and [`OneWorkspace`], the single exception
//! that answers honestly instead, is defined beside the stubs it must not
//! be confused with.

use afd_api::services::{
    DeviceFlow, ModelCatalogue, TenantBilling, TenantKeys, TenantWorkspaces, TerminalCredentials,
    WorkspaceOwnership,
};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_tenant::session::input as session_input;

/// A login surface with no queue behind it.
///
/// Every verb answers the refusal a queue that would not answer produces, and
/// that is the honest stub rather than a lazy one: a device-flow verb's whole
/// behaviour lives in a Lua script evaluated by a real Redis, so there is no
/// success this could invent that would not be inventing the state machine too.
/// What a suite here proves is the guard, the credential-class narrowing and
/// the refusal matrix in FRONT of the verb — for which reaching the handler at
/// all is the assertion.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoLogins;

impl NoLogins {
    /// The refusal every verb below answers with.
    fn unavailable<T>() -> afd_tenant::Result<T> {
        Err(afd_tenant::Error::queue_unavailable())
    }
}

impl DeviceFlow for NoLogins {
    fn open(
        &self,
        _opening: &session_input::Opening<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::session::Opened>> + Send {
        std::future::ready(Self::unavailable())
    }

    fn poll(
        &self,
        _session_id: &str,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::session::Waiting>> + Send {
        std::future::ready(Self::unavailable())
    }

    fn approve(
        &self,
        _session_id: &str,
        _approval: &session_input::Approval<'_>,
        _approver: &str,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        std::future::ready(Self::unavailable())
    }

    fn verify(
        &self,
        _session_id: &str,
        _code: &session_input::Code<'_>,
        _fingerprint: &afd_tenant::session::Fingerprint,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::session::Redeemed>> + Send {
        std::future::ready(Self::unavailable())
    }

    fn cancel(
        &self,
        _session_id: &str,
        _owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::session::Cancelled>> + Send {
        std::future::ready(Self::unavailable())
    }

    fn cancel_all(
        &self,
        _owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<Vec<String>>> + Send {
        std::future::ready(Self::unavailable())
    }
}

/// The identifier of the one workspace [`OneWorkspace`] answers for.
///
/// A constant rather than a fixture, so a suite asserting the DENIED half can
/// name a workspace it knows is foreign without coordinating with the allow
/// half. Any other well-formed identifier is somebody else's.
pub(crate) const OWNED_WORKSPACE: &str = "01924f4e-0000-7000-8000-00000000beef";

/// A workspace-ownership resolver that owns exactly one workspace.
///
/// Unlike [`NoWork`], this one answers HONESTLY rather than uniformly, and it
/// has to: the layer it feeds is the thing under test in the router's refusal
/// matrix, and a stub that allowed everything would make the deny path
/// unreachable while a stub that denied everything would make every workspace
/// handler unreachable. Owning one and refusing the rest gives the suite both
/// halves with no Postgres in it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OneWorkspace;

impl WorkspaceOwnership for OneWorkspace {
    fn authorize(
        &self,
        principal: &afd_auth::principal::Principal,
        workspace: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        // A runner has no tenant authority, exactly as in production: the
        // statement binds nothing that could match, so the answer is a denial
        // rather than an error.
        let tenant = principal.tenant().cloned();
        let owned = workspace.as_str() == OWNED_WORKSPACE;
        std::future::ready(Ok(tenant.filter(|_| owned)))
    }

    fn tenant_of(
        &self,
        principal: &afd_auth::principal::Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        std::future::ready(Ok(principal.tenant().cloned()))
    }
}

/// The deployment every fixture credential records.
pub(crate) const DEPLOYMENT: &str = "https://api.fixture.test";

/// A command-line credential store with no Postgres behind it.
///
/// Every verb answers the refusal a datastore that would not answer produces,
/// for [`NoKeys`]' reason: the mint's whole behaviour is a transaction a real
/// Postgres evaluates — an advisory lock, a scoped revoke and an insert the
/// partial unique index arbitrates — so there is no success this could invent
/// without inventing that too. What a suite here proves is the principal-mode
/// refusals in FRONT of the verb, which is exactly where this family's rules
/// live.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoTerminals;

impl TerminalCredentials for NoTerminals {
    fn user_of(
        &self,
        _subject: &str,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::cli_credential::UserIdentity>> + Send
    {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn mint(
        &self,
        _request: &afd_tenant::cli_credential::MintRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::cli_credential::Revealed>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn revoke(
        &self,
        _user: &Uuid7,
        _credential: &Uuid7,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::cli_credential::Revoked>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }
}

/// A workspace directory with no Postgres behind it.
///
/// Both verbs answer the refusal a datastore that would not answer produces,
/// for [`NoKeys`]' reason: the page is a statement and the create is an
/// insert a unique index arbitrates, so there is no success this could invent
/// that would not be inventing the rows too. What a suite here proves is the
/// guard, the tenant resolution and the query-string refusals in FRONT of the
/// verbs — deliberately DISTINCT from [`OneWorkspace`], which answers the
/// ownership seam honestly so the deny half of that matrix stays reachable.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoDirectory;

impl TenantWorkspaces for NoDirectory {
    fn page(
        &self,
        _tenant: &Uuid7,
        _filter: Option<&str>,
        _after: Option<&afd_tenant::workspace::directory::After>,
        _limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::workspace::directory::WorkspacePage>> + Send
    {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn create(
        &self,
        _tenant: &Uuid7,
        _chosen: Option<afd_tenant::workspace::name::Chosen>,
        _created_by: &str,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::workspace::directory::Created>> + Send
    {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }
}

/// A model catalogue with no Postgres behind it.
///
/// One verb, one honest refusal, for the reason every stub here refuses: the
/// page is a statement a real Postgres evaluates. What a suite proves is the
/// guard and the query-string refusals in FRONT of it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoModels;

impl ModelCatalogue for NoModels {
    fn page(
        &self,
        _filter: Option<&str>,
        _after: Option<&afd_tenant::models::Boundary>,
        _limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::models::LibraryPage>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }
}

/// A billing read surface with no Postgres behind it.
///
/// Both verbs answer the refusal a datastore that would not answer produces,
/// for [`NoKeys`]' reason: the reads' whole behaviour is two statements a real
/// Postgres evaluates — including the missing-wallet invariant, which only a
/// seeded database can distinguish from an empty answer — so there is no
/// success this could invent that would not be inventing the rows too. What a
/// suite here proves is the guard, the tenant resolution and the query-string
/// refusals in FRONT of the verb.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoBilling;

impl TenantBilling for NoBilling {
    fn snapshot(
        &self,
        _tenant: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::billing::Wallet>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn charges(
        &self,
        _tenant: &Uuid7,
        _limit: u32,
        _boundary: Option<&afd_tenant::billing::cursor::Boundary>,
    ) -> impl Future<Output = afd_tenant::Result<Vec<afd_tenant::billing::ChargeRow>>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }
}

/// An api-key store with no Postgres behind it.
///
/// Every verb answers the refusal a datastore that would not answer produces,
/// for the reason [`NoLogins`] does: the lifecycle's whole behaviour is in two
/// CTEs a real Postgres evaluates, so there is no success this could invent
/// that would not be inventing the state machine too. What a suite here proves
/// is the guard, the tenant resolution and the refusal matrix in FRONT of the
/// verb.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoKeys;

impl TenantKeys for NoKeys {
    fn mint(
        &self,
        _request: &afd_tenant::apikey::MintRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::apikey::Revealed>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn list(
        &self,
        _tenant: &Uuid7,
        _page: &afd_core::paging::Page<afd_tenant::apikey::ApiKeySort>,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::apikey::Listing>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn revoke(
        &self,
        _tenant: &Uuid7,
        _key: &Uuid7,
        _intent: afd_tenant::apikey::Deactivation,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<afd_tenant::apikey::Revoked>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }

    fn delete(
        &self,
        _tenant: &Uuid7,
        _key: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        std::future::ready(Err(afd_tenant::Error::datastore_unavailable()))
    }
}
