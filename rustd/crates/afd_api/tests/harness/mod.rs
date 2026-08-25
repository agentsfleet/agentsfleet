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
use axum::Router;
use axum::body::Body;
use axum::response::Response;
use http::{Method, Request};
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
    now: UnixMillis,
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

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn runners(&self) -> &Runners {
        &self.runners
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
