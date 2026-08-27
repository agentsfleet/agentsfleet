//! A production router over seams a test drives, and no datastore anywhere.
//!
//! The exonum testkit pattern the milestone names: real HTTP through the real
//! router, over the real layers, with the things that reach a network replaced
//! at their declared seams. What that buys is that the refusal matrix is proven
//! against the thing that ships — including the layer ORDER, which a test
//! calling a handler directly cannot see at all.
//!
//! # Why the pool is real and unreachable rather than mocked
//!
//! There is no seam between `afd_fleet` and Postgres, deliberately: the
//! statements are the parity target, so a fake store would prove a handler
//! against SQL nobody runs. Instead the store holds a pool over an address that
//! answers nothing ([`afd_db::Db::unreachable`]), which is exactly what a
//! datastore outage looks like from the request path — and lets a suite prove
//! the transport-class refusal (RULE ECL) without stopping a container.
//!
//! A test that needs rows is an integration test, `#[ignore]`d and run by
//! `make test-integration-rustd`. This harness is for everything BEFORE the
//! first row is read, which is where §1's dimensions live.
#![allow(
    dead_code,
    reason = "shared across suites; each uses the subset its dimensions need"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_api::router::{Dependencies, ReadyInputs, build};
use afd_api::services::{DeviceFlow, Leasing, TenantKeys, TerminalCredentials, WorkspaceOwnership};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, Planes, Services};
use afd_auth::credential::{CredentialKind, Presented};
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_auth::verifier::NoVerifier;
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_fleet::Runners;
use afd_fleet::bundle::{Bundles, ContentHash};
use afd_tenant::session::input as session_input;
use axum::Router;
use axum::body::Body;
use axum::response::Response;
use bytes::Bytes;
use http::{Method, Request};
use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;
use serde_json::Value;
use tower::ServiceExt as _;

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A fixed instant, so every row a verb writes is stamped predictably.
const FROZEN: i64 = 1_760_000_000_000;

/// The seams a suite arranges, and the state the router is built over.
#[derive(Debug)]
pub(crate) struct Fleet {
    ready: ReadyInputs,
    directory: MockDirectory,
    capabilities: MockCapabilities,
    authenticator: Planes<MockDirectory, MockCapabilities, NoVerifier>,
    runners: Runners,
    leases: NoWork,
    bundles: Bundles,
    workspaces: OneWorkspace,
    api_keys: NoKeys,
    now: UnixMillis,
}

/// A lease plane that always answers no-work.
///
/// The production plane holds a Redis connection that is opened by CONNECTING,
/// so these suites cannot build one — and should not: what they prove is the
/// router's guard, scope and refusal matrix, which is decided BEFORE any verb
/// runs. A stub that always answers the same thing keeps that boundary honest,
/// because a suite here cannot accidentally start asserting on lease
/// behaviour that belongs to `afd_fleet`'s own integration lane.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoWork;

impl Leasing for NoWork {
    fn lease(
        &self,
        _runner_id: &Uuid7,
        _degraded: bool,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<String>> + Send {
        std::future::ready(Ok(r#"{"lease":null,"retry_after_ms":1000}"#.to_owned()))
    }

    /// Accepts every report and charges nothing, which is what a plane with no
    /// work in it would do.
    ///
    /// Deliberately not a refusal. A suite here proves the guard, scope and
    /// refusal matrix in FRONT of the verb, so what it needs is for an
    /// authenticated runner to REACH the handler — and every refusal this verb
    /// can raise needs a real lease row to be refused against, which is
    /// `afd_fleet`'s integration lane and its live Postgres. Returning an error
    /// here would put a code on the wire that no datastore decided, and a
    /// router suite asserting on it would be asserting on this stub.
    fn report(
        &self,
        _runner_id: &Uuid7,
        _request: &afd_wire::report::ReportRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::money::Nanos>> + Send {
        std::future::ready(Ok(afd_fleet::money::Nanos::ZERO))
    }

    /// Accepts every batch of frames and publishes none, which is what a plane
    /// with no queue behind it does.
    ///
    /// The truest of the three stubs: publishing IS best-effort in production,
    /// so a plane that drops every frame and answers `Ok` is not pretending —
    /// it is one end of the range the real verb already spans.
    fn activity(
        &self,
        _runner_id: &Uuid7,
        _lease_id: &str,
        _frames: &[afd_wire::activity::ActivityFrame<'_>],
    ) -> impl Future<Output = afd_fleet::Result<()>> + Send {
        std::future::ready(Ok(()))
    }

    /// Mints nothing, and says so with the code a deployment holding no
    /// platform credential answers.
    ///
    /// A REFUSAL where the three stubs above answer `Ok`, and the asymmetry is
    /// the verb's: `mint` has no success this suite could assert without a
    /// vault row, a grant and a vendor, so an `Ok` here would have to invent a
    /// token. `UZ-CRED-002` is the honest answer for a plane with no platform
    /// credentials in it — the same one production gives — and it still proves
    /// what these suites are for: that an authenticated runner REACHES the
    /// handler and an unauthenticated one does not.
    fn mint(
        &self,
        _runner_id: &Uuid7,
        _request: &afd_wire::credentials::MintCredentialRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::credential::Minted>> + Send {
        std::future::ready(Err(afd_fleet::Error::mint_unconfigured()))
    }

    /// Hydrates nothing, which is what a fleet that has never run remembers.
    ///
    /// An empty window is a real answer, not a stand-in: a first run seeds from
    /// exactly this.
    fn hydrate(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<Vec<afd_wire::memory::MemoryDelta<'static>>>> + Send
    {
        std::future::ready(Ok(Vec::new()))
    }

    /// Stores nothing and says so, for the reason [`NoWork::report`] accepts.
    fn capture(
        &self,
        _runner_id: &Uuid7,
        _fleet_id: &Uuid7,
        _request: &afd_wire::memory::MemoryPushRequest<'_>,
        _now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<afd_fleet::memory::Captured>> + Send {
        std::future::ready(Ok(afd_fleet::memory::Captured::default()))
    }

    /// Renews to the instant asked about, for the reason [`NoWork::report`]
    /// accepts.
    fn renew(
        &self,
        _runner_id: &Uuid7,
        _lease_id: &str,
        _request: afd_wire::report::RenewRequest,
        now: UnixMillis,
    ) -> impl Future<Output = afd_fleet::Result<UnixMillis>> + Send {
        std::future::ready(Ok(now))
    }
}

impl Fleet {
    /// An instance whose dependencies answer, whose directory is empty, and
    /// whose Postgres is not there.
    pub(crate) fn new() -> Self {
        let directory = MockDirectory::new();
        let capabilities = MockCapabilities::new();
        let environment = MapEnv::from_pairs([(DbRole::Api.url_knob(), NOWHERE)]);
        let pool = PoolConfig::resolve(&environment, DbRole::Api)
            .expect("the fixture connection string is well formed");
        Self {
            ready: ReadyInputs {
                database: true,
                queue: true,
            },
            authenticator: Planes::new(directory.clone(), capabilities.clone(), NoVerifier),
            directory,
            capabilities,
            runners: Runners::new(Db::unreachable(&pool), Entropy::new()),
            leases: NoWork,
            // Unconfigured by default, so a suite that says nothing about
            // snapshots proves the refusal a deployment with no R2 knobs gives
            // — which is most of them.
            bundles: Bundles::unconfigured(),
            workspaces: OneWorkspace,
            api_keys: NoKeys,
            now: UnixMillis::from_millis(FROZEN),
        }
    }

    /// An instance reporting `ready` to `/readyz`.
    pub(crate) const fn reporting(mut self, ready: ReadyInputs) -> Self {
        self.ready = ready;
        self
    }

    /// Files a runner row under the digest of `token`.
    pub(crate) fn with_runner(self, token: &str, runner: &Uuid7, live: Liveness) -> Self {
        file_runner(&self.directory, token, runner, live);
        self
    }

    /// Files a person row under the digest of `key`, holding `scopes`.
    pub(crate) fn with_person(mut self, key: &str, subject: &str, scopes: ScopeSet) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        self.directory = self.directory.with(
            CredentialKind::TenantApiKey,
            &presented(key),
            CredentialRecord::Person {
                tenant: tenant(),
                subject: who.clone(),
                live: Liveness::Live,
            },
        );
        self.capabilities = self.capabilities.with(&who, scopes);
        self
    }

    /// Files a person row under the digest of an `afc_` command-line credential.
    ///
    /// Sibling of [`Self::with_person`], and the difference is the whole point:
    /// the two resolve to the same person with the same capabilities and differ
    /// only in credential CLASS, which is exactly the axis the command-line
    /// credential routes refuse on. A suite cannot prove that rule with one of
    /// them.
    pub(crate) fn with_terminal(
        mut self,
        credential: &str,
        subject: &str,
        scopes: ScopeSet,
    ) -> Self {
        let who = Subject::new(subject).expect("the fixture subject is not blank");
        self.directory = self.directory.with(
            CredentialKind::CliCredential,
            &presented(credential),
            CredentialRecord::Person {
                tenant: tenant(),
                subject: who.clone(),
                live: Liveness::Live,
            },
        );
        self.capabilities = self.capabilities.with(&who, scopes);
        self
    }

    /// Backs this instance with an in-memory snapshot store holding `body`
    /// under `content_hash`.
    ///
    /// `object_store::memory::InMemory` rather than a mock of our own: it is
    /// the backend the workspace manifest names for exactly this, so what the
    /// suite drives is the same client production drives with a different
    /// backing store — not a second implementation that could agree with the
    /// test and disagree with R2.
    ///
    /// Async because a `put` is, which is why it is not one of the `const`
    /// builders above.
    pub(crate) async fn with_snapshot(mut self, content_hash: &str, body: &[u8]) -> Self {
        let store = InMemory::new();
        let hash = ContentHash::parse(content_hash).expect("the fixture digest is well formed");
        store
            .put(&hash.snapshot_key(), Bytes::copy_from_slice(body).into())
            .await
            .expect("an in-memory put cannot fail");
        self.bundles = Bundles::new(Arc::new(store));
        self
    }

    /// The directory, for a suite that revokes between two requests.
    pub(crate) const fn directory(&self) -> &MockDirectory {
        &self.directory
    }

    /// The capability source, for a suite that narrows a subject.
    pub(crate) const fn capabilities(&self) -> &MockCapabilities {
        &self.capabilities
    }

    /// The production router, over this instance.
    pub(crate) fn router(self) -> Router {
        let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
        build(Arc::new(self), &admission)
    }
}

impl Dependencies for Fleet {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        std::future::ready(self.ready)
    }
}

impl Services for Fleet {
    type Auth = Planes<MockDirectory, MockCapabilities, NoVerifier>;
    type Leases = NoWork;
    type Sessions = NoLogins;
    type Workspaces = OneWorkspace;
    type ApiKeys = NoKeys;
    type CliCredentials = NoTerminals;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn runners(&self) -> &Runners {
        &self.runners
    }

    fn leases(&self) -> &NoWork {
        &self.leases
    }

    fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    fn sessions(&self) -> &NoLogins {
        &NoLogins
    }

    fn workspaces(&self) -> &OneWorkspace {
        &self.workspaces
    }

    fn api_keys(&self) -> &NoKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &NoTerminals {
        &NoTerminals
    }

    /// A fixed deployment, which is what a real one is too.
    ///
    /// Read from configuration in the binary rather than from the request, so a
    /// constant here is the same KIND of value the daemon serves with — not a
    /// simplification a suite would have to remember is one.
    fn deployment(&self) -> &str {
        DEPLOYMENT
    }

    fn now(&self) -> UnixMillis {
        self.now
    }
}

/// The tenant every fixture person acts in.
pub(crate) fn tenant() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-000000000001").expect("the fixture tenant is canonical")
}

/// A runner identifier a fixture files a row under.
pub(crate) fn runner_id() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000a1").expect("the fixture runner is canonical")
}

/// Files a runner row, replacing whatever was under that credential.
///
/// Takes the directory by reference and clones inside, because `MockDirectory`
/// is a builder over shared state: `with` mutates the state every clone points
/// at and then hands the handle back. A suite revoking between two requests
/// wants the mutation and not the handle, and saying so once here keeps a
/// discarded return value out of every test that does it.
pub(crate) fn file_runner(directory: &MockDirectory, token: &str, runner: &Uuid7, live: Liveness) {
    let _filed = directory.clone().with(
        CredentialKind::RunnerToken,
        &presented(token),
        CredentialRecord::Machine {
            runner: runner.clone(),
            degraded: false,
            live,
        },
    );
}

/// A credential as the directory keys it — by the digest of what is PRESENTED,
/// so a fixture names the value a test will actually send.
fn presented(raw: &str) -> Presented {
    Presented::from_authorization(&format!("Bearer {raw}"))
        .expect("a fixture credential is never blank")
}

/// One request at `router`, with an optional credential.
pub(crate) async fn send(
    router: &Router,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> Response {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = credential {
        request = request.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request
        .body(Body::from(body.to_owned()))
        .expect("the test request is well formed");
    router
        .clone()
        .oneshot(request)
        .await
        .expect("axum is infallible")
}

/// Reads a response body back as JSON.
pub(crate) async fn json_body(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test response body is small and in memory");
    serde_json::from_slice(&bytes).expect("the response must be valid JSON")
}

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
