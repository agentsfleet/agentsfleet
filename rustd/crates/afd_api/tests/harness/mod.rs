//! A production router over seams a test drives, and no datastore anywhere.
//!
//! The exonum testkit pattern the milestone names: real HTTP through the real
//! router, over the real layers, with the things that reach a network replaced
//! at their declared seams. What that buys is that the refusal matrix is proven
//! against the thing that ships — including the layer ORDER, which a test
//! calling a handler directly cannot see at all.
//!
//! # Every store is the REAL one, over datastores that answer nothing
//!
//! There is no seam between a store and its statements, deliberately: the SQL
//! is the parity target, so a fake store would prove a handler against SQL
//! nobody runs. Each store here therefore holds a pool over an address that
//! answers nothing ([`afd_db::Db::unreachable`]) and, where it needs one, a
//! queue built the same way ([`afd_redis::Redis::unreachable`]). That is
//! exactly what a datastore outage looks like from the request path, and it
//! lets a suite prove the transport-class refusal (RULE ECL) without stopping a
//! container.
//!
//! This replaced eight hand-written `No*` stubs, one per service seam, each
//! spelling `Err(Error::datastore_unavailable())` in every method. They were
//! not wrong, they were REDUNDANT — and worse than redundant in one specific
//! way: a stub that INVENTS the refusal keeps agreeing with the suite after the
//! real store stops producing it. Deleting them also deleted the
//! `test-util` constructors that existed only to fabricate that error.
//!
//! Two stubs survive, and neither is a uniform refusal — which is the line:
//! [`OneWorkspace`] answers ownership honestly so both halves of that matrix
//! stay reachable, and [`NoWork`] answers a lease plane's verbs with success so
//! the runner routes are reachable at all. Each carries test LOGIC no real
//! store over a dead datastore could stand in for.
//!
//! A test that needs rows is an integration test, `#[ignore]`d and run by
//! `make test-integration-rustd`. This harness is for everything BEFORE the
//! first row is read.
#![allow(
    dead_code,
    unused_imports,
    reason = "shared across suites; each uses the subset its dimensions need"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_api::router::{Dependencies, ReadyInputs, build};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, Planes, SchedulePlane, Services};
use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Digest, Liveness};
use afd_auth::error::Unavailable;
use afd_auth::mock::{MockCapabilities, MockDirectory, MockVerifier};
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_auth::verifier::VerifyError;
use afd_billing::tenant::Billing;
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_credential::provider::Providers;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_events::{History, Steer};
use afd_fleet::bundle::{Bundles, ContentHash};
use afd_fleet::memory::Memories;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_observability::Analytics;
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_runner::Runners;
use afd_sse::{Ceiling, Live};
use afd_state::Credentials;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
// Aliased for the reason the composition root aliases it: `afd_credential::vault`
// is the runner plane's reader and this is the workspace-admin surface.
use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_approval::{Inbox, IntegrationGrants};
use afd_tenant::preference::Preferences;
use afd_vault::Vault as SecretVault;
use axum::Router;
use bytes::Bytes;
use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;

mod readiness;
mod stubs_identity;
mod stubs_ingress;
mod stubs_provider;

use self::readiness::{NOWHERE_GITHUB, unreachable_pool, unreachable_queue};
mod stubs_runner;
mod stubs_tenant;
mod support;

/// Signed deliveries, as a provider would present them.
pub(crate) mod webhook;

pub(crate) use self::stubs_identity::{RecordingWriteback, WroteBack};
pub(crate) use self::stubs_ingress::{HarnessIngress, Recorded, Scripted};
pub(crate) use self::stubs_provider::HarnessProviders;
pub(crate) use self::stubs_runner::NoWork;
pub(crate) use self::stubs_tenant::{DEPLOYMENT, OWNED_WORKSPACE, OneWorkspace};
/// Where this fixture deployment's schedule fires would arrive.
///
/// A real destination shape, because it is half of what a fire token's subject
/// is checked against — a blank one would make every token fail the subject
/// check for a reason no test was about.
pub(crate) const SCHEDULE_DESTINATION: &str =
    "https://api.fixture.test/v1/ingress/qstash/schedules";

/// Which scheduler these fixtures talk to.
///
/// A `.test` host, deliberately: `.test` is reserved and resolves nowhere, so a
/// harness suite that reaches the network by accident fails as a connection
/// error naming this constant. The alternative — letting the client fall back to
/// [`afd_cron::qstash::API_BASE`] — would point a test suite at the
/// vendor's real US deployment.
pub(crate) const SCHEDULE_API_BASE: &str = "https://qstash.fixture.test/v2";

pub(crate) use self::support::{
    connect_redis, file_runner, json_body, presented, redis_config, runner_id, send,
    send_with_headers, tenant,
};

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A Redis nobody is listening on, for the same reason and on the same port.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The pool knob naming how long an acquire may spend before it reports.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// What this harness sets it to — see [`unreachable_pool`].
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// A fixed instant, so every row a verb writes is stamped predictably.
const FROZEN: i64 = 1_760_000_000_000;

/// Milliseconds in a second, named because two units meet here.
const MILLIS_PER_SECOND: i64 = 1_000;

/// [`FROZEN`] as whole seconds — the unit a signed timestamp header carries.
///
/// A signature scheme that binds a timestamp is checked against
/// `services.now()`, which is this frozen instant and not the wall clock. A
/// test that built its timestamps from `SystemTime::now` would be sixty-odd
/// million seconds adrift and read every delivery as stale, whatever it was
/// actually testing.
pub(crate) const fn frozen_unix_seconds() -> i64 {
    FROZEN / MILLIS_PER_SECOND
}

/// The process key the secret store seals under.
///
/// Every router built by [`Fleet::new`] refuses at the pool before an envelope
/// is built, so for those this is only the key a `Vault` cannot be CONSTRUCTED
/// without — the invariant that type exists to carry. [`Fleet::live`] does
/// reach a vault, and a suite there seals through [`vault`] under this same
/// key, which is what makes a secret it stores openable by the router.
const FIXTURE_KEK: [u8; 32] = [0x11; 32];

/// A vault over `database`, sealing under the key the live routers open with.
///
/// A suite proving what a route does with a secret has to PUT one somewhere
/// first, and the only writer that produces a row the router can open is one
/// holding [`FIXTURE_KEK`]. Handing that key out instead would let a suite
/// build a vault under a different one and watch every read answer `None` —
/// which reads as "the route refuses unconfigured" and proves nothing.
pub(crate) fn vault(database: Db) -> SecretVault {
    SecretVault::new(
        database,
        Arc::new(Kek::from_bytes(FIXTURE_KEK)),
        Entropy::new(),
    )
}

/// The pepper the device-flow code digest is computed under, for the same reason.
const FIXTURE_PEPPER: &[u8] = b"fixture-session-code-pepper";

/// The dashboard origin a login surface composes approval links against.
const FIXTURE_APP_URL: &str = "https://app.fixture.test";

/// The seams a suite arranges, and the state the router is built over.
#[derive(Debug)]
pub(crate) struct Fleet {
    ready: ReadyInputs,
    mock_directory: MockDirectory,
    directory: Directory,
    capabilities: MockCapabilities,
    authenticator: Planes<Directory, MockCapabilities, MockVerifier>,
    runners: Runners,
    leases: NoWork,
    bundles: Bundles,
    workspaces: OneWorkspace,
    workspace_directory: Workspaces,
    api_keys: ApiKeys,
    cli_credentials: CliCredentials,
    logins: Logins,
    fleets: Fleets,
    secrets: SecretVault,
    ingress: HarnessIngress,
    signup_writeback: RecordingWriteback,
    schedules: SchedulePlane,
    connectors: afd_connector::Connectors,
    schedule_keys: Option<afd_cron::SigningKeys>,
    platform_admin: Option<Uuid7>,
    /// Opening a personal account, over whatever pool this fixture holds.
    signups: afd_tenant::signup::Signups,
    /// What a signup event is verified against — `None` refuses every one.
    identity_webhook_secret: Option<afd_crypto::secret::SecretBytes>,
    /// The dashboard base a connect relays through.
    ///
    /// A field rather than the constant so ONE case can make it unusable. Every
    /// other fixture keeps `FIXTURE_APP_URL`, because a base that is not a URL
    /// makes every connect refuse for a reason that test was not about — which
    /// is exactly why the refusal needs its own case rather than a shared one.
    dashboard_base: String,
    preferences: Preferences,
    approvals: Inbox,
    grants: IntegrationGrants,
    events: History,
    live: Live,
    analytics: Analytics,
    steering: Steer,
    memories: Memories,
    billing: Billing,
    catalogue: Models,
    /// The tenant provider store: the same production value, over a pool that
    /// answers nothing and a fixture key that opens nothing.
    providers: HarnessProviders,
    runner_lease_history: RunnerLeaseHistory,
    admin_models: AdminModels,
    platform_keys: PlatformKeys,
    libraries: Libraries,
    library_imports: LibraryImports,
    now: UnixMillis,
}

/// The same auth seam backed either by the fast map or production Postgres.
///
/// `Fleet::new` files credentials into the map, which needs no datastore; the
/// live-router suites resolve the same seam through real Postgres so a scope
/// gate is proven against the rows a migration actually created.
#[derive(Debug, Clone)]
pub(crate) enum Directory {
    Mock(MockDirectory),
    Live(Credentials),
}

impl CredentialDirectory for Directory {
    async fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> Result<Option<CredentialRecord>, Unavailable> {
        match self {
            Self::Mock(directory) => directory.resolve(kind, digest).await,
            Self::Live(directory) => directory.resolve(kind, digest).await,
        }
    }
}

/// How many streams a fixture instance carries.
///
/// Above anything a suite opens, so a stream refused in a test is refused by
/// the thing that test is about. The one suite that DOES prove the ceiling
/// lowers it with [`Fleet::carrying_at_most`].
const DEFAULT_STREAM_CEILING: usize = 64;

mod fleet;
mod fleet_seams;

mod services;
