//! §1 — the runner plane's guard: who is admitted, who is refused, and how.
//!
//! Every case here is proven through the PRODUCTION router, over real HTTP,
//! with no datastore behind it. That is deliberate and it is what the layer
//! ordering makes necessary: admission, then the credential, then the
//! capability, then the handler. A test that called a handler directly would
//! prove none of those four are in the right order, which is the only part of
//! this that a wiring mistake can break.
//!
//! # What "admitted" looks like with no Postgres
//!
//! A request that clears both checks reaches its handler, which acquires a
//! connection, which fails — so the answer is `UZ-INTERNAL-001`, a 503. That is
//! not a weaker assertion than a 200; it is a DIFFERENT one, and it is the one
//! that matters here: the request got past the guard. Whether the handler then
//! writes the right rows is `test_lease_writes_row_parity`'s question, and it
//! runs against a live datastore in `make test-integration-rustd`.
//!
//! The 503 is also load-bearing on its own (RULE ECL): a runner counts
//! consecutive REJECTIONS toward a self-termination ceiling and resets that
//! counter on a transport failure, so a datastore outage answered as a 401
//! walks a healthy fleet's runners to shutdown one beat at a time.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::directory::Liveness;
use afd_auth::scope::{RUNNER_SCOPES, Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, file_runner, json_body, runner_id, send};

/// A well-formed runner credential: the marker and sixty-four hex characters.
const RUNNER_TOKEN: &str = "agt_r0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A second one, belonging to no row.
const UNKNOWN_TOKEN: &str = "agt_rffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// A well-formed TENANT credential, for the boundary cases.
const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The runner plane's own paths.
const SELF_PATH: &str = "/v1/runners/me";
const HEARTBEAT_PATH: &str = "/v1/runners/me/heartbeats";
const ENROL_PATH: &str = "/v1/runners";

/// The digest a bundle fixture is stored under: SHA-256 of the empty input,
/// which is canonical rather than invented.
const BUNDLE_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The identity-provider subject a fixture operator acts as.
const OPERATOR: &str = "user_2abcOPERATOR";

/// The registry code a refusal carries.
async fn code_of(response: axum::response::Response) -> String {
    json_body(response)
        .await
        .get("error_code")
        .and_then(serde_json::Value::as_str)
        .expect("every problem envelope carries an error code")
        .to_owned()
}

/// Dimension 1.1 — every state a presented runner credential can be in.
///
/// The four rows are the matrix `runner_bearer.zig` spells across three `if`
/// chains: a credential that matches nothing, one whose row is no longer live,
/// one from the other plane entirely, and none at all. All four answer 401, and
/// the CODE is what tells them apart — which is the part a runner branches on.
#[tokio::test]
async fn test_runner_bearer_state_matrix() {
    let fleet = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_runner(UNKNOWN_TOKEN, &runner_id(), Liveness::Revoked);
    let router = fleet.router();

    // A live row: past the guard, into the handler, onto the missing datastore.
    let admitted = send(&router, Method::GET, SELF_PATH, Some(RUNNER_TOKEN), "").await;
    assert_eq!(
        admitted.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a live runner must reach its handler; the 503 is the absent datastore, \
         not the guard"
    );
    assert_eq!(code_of(admitted).await, "UZ-INTERNAL-001");

    // A row that exists and is revoked, cordoned, draining or deleted. Its own
    // code, because "stop presenting this" and "this was never valid" are
    // different instructions to a host.
    let blocked = send(&router, Method::GET, SELF_PATH, Some(UNKNOWN_TOKEN), "").await;
    assert_eq!(blocked.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(blocked).await, "UZ-RUN-009");

    // A credential no row matches, and a request with none at all. The same
    // answer for both: an unauthenticated caller must not learn which of their
    // guesses was closer.
    let missing = "agt_r1111111111111111111111111111111111111111111111111111111111111111";
    for credential in [Some(missing), None] {
        let refused = send(&router, Method::GET, SELF_PATH, credential, "").await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(code_of(refused).await, "UZ-RUN-001");
    }
}

/// Dimension 1.1 (boundary) — neither plane accepts the other's credential.
///
/// `docs/AUTH.md` states this as a rule about which middleware is mounted
/// where; here it is a property of the route table, so a route mounted one
/// family too wide fails this rather than shipping.
///
/// The CODES differ by plane on purpose. A tenant key on the runner plane
/// answers `UZ-RUN-001`, because a runner client classifies its own plane's
/// codes and has no branch for the tenant plane's.
#[tokio::test]
async fn test_planes_refuse_each_others_credentials() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerEnroll]),
        )
        .router();

    let wrong_plane = send(&router, Method::GET, SELF_PATH, Some(TENANT_KEY), "").await;
    assert_eq!(wrong_plane.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        code_of(wrong_plane).await,
        "UZ-RUN-001",
        "the runner plane refuses in its own vocabulary, before any lookup"
    );

    let wrong_way = send(&router, Method::POST, ENROL_PATH, Some(RUNNER_TOKEN), "{}").await;
    assert_eq!(wrong_way.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        code_of(wrong_way).await,
        "UZ-AUTH-002",
        "a runner token is not a tenant credential, and the tenant plane says so \
         in the tenant plane's code"
    );
}

/// Dimension 1.2 — a datastore that will not answer is a 503, never a 401.
///
/// The distinction the whole `CredentialDirectory` seam exists for. Collapsing
/// them would report a Postgres outage as an authentication rejection, and the
/// runner client counts rejections toward a self-termination ceiling.
#[tokio::test]
async fn test_runner_auth_pg_outage_503() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    fleet.directory().set_unavailable(true);
    let router = fleet.router();

    let outage = send(&router, Method::GET, SELF_PATH, Some(RUNNER_TOKEN), "").await;

    assert_eq!(
        outage.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a lookup that could not run is a transport failure; a 401 here walks a \
         healthy fleet to shutdown"
    );
    assert_eq!(code_of(outage).await, "UZ-AUTH-004");
}

/// Dimension 1.3 — enrolment is gated on `runner:enroll` and nothing weaker.
///
/// The grant is held independently of `runner:read` and `runner:write` because
/// the host it creates receives every tenant's inline secrets. A caller holding
/// the rest of the runner rungs is still refused.
#[tokio::test]
async fn test_register_enroll_gate() {
    let without = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead, Scope::RunnerWrite]),
        )
        .router();

    let denied = send(&without, Method::POST, ENROL_PATH, Some(TENANT_KEY), "{}").await;
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "403, not 401: the caller proved who they are, so re-authenticating \
         cannot help and a 401 would loop them"
    );
    assert_eq!(code_of(denied).await, "UZ-AUTH-022");

    let with = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerEnroll]),
        )
        .router();

    let admitted = send(&with, Method::POST, ENROL_PATH, Some(TENANT_KEY), "{}").await;
    assert_eq!(
        admitted.status(),
        StatusCode::BAD_REQUEST,
        "the scope holder is past the gate — what refuses now is the body, \
         which is the handler's judgement rather than the guard's"
    );
    assert_eq!(code_of(admitted).await, "UZ-REQ-001");
}

/// Dimension 1.4 — a revoked runner is refused on its very next request.
///
/// There is no memoised verdict to invalidate, because there is no memo: the
/// row is read on EVERY request, and that read is the revocation channel. The
/// lookup count is asserted for the same reason — a cache would show up here as
/// one lookup for two requests, on any replica.
#[tokio::test]
async fn test_revocation_immediate() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let directory = fleet.directory().clone();
    let router = fleet.router();

    let before = send(&router, Method::GET, SELF_PATH, Some(RUNNER_TOKEN), "").await;
    assert_eq!(
        before.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the live runner is admitted"
    );

    // The operator cordons the host. Nothing is told to the daemon.
    file_runner(&directory, RUNNER_TOKEN, &runner_id(), Liveness::Revoked);

    let after = send(&router, Method::GET, SELF_PATH, Some(RUNNER_TOKEN), "").await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(after).await, "UZ-RUN-009");
    assert_eq!(
        directory.lookups(),
        2,
        "one lookup per request. Fewer means a verdict was cached, and a cached \
         verdict is a revocation that does not take effect"
    );
}

/// A heartbeat is admitted on the same terms as every other runner verb.
///
/// The heartbeat REPLY is unconditionally `ok`, so the only thing that can
/// refuse a beat is the guard — which makes this the one route where the guard
/// is the entire access-control story.
#[tokio::test]
async fn test_heartbeat_is_guarded_like_every_other_runner_verb() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    let refused = send(&router, Method::POST, HEARTBEAT_PATH, None, "{}").await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(refused).await, "UZ-RUN-001");

    let admitted = send(
        &router,
        Method::POST,
        HEARTBEAT_PATH,
        Some(RUNNER_TOKEN),
        "{}",
    )
    .await;
    assert_eq!(
        admitted.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "past the guard, into the policy read, onto the missing datastore"
    );
}

/// Dimension 4.4 — a bundle fetch streams the exact stored bytes, and an
/// unknown hash is a 404.
///
/// The one verb on this plane that answers bytes rather than JSON, so what it
/// must not do is interpret them: the fixture is deliberately neither valid tar
/// nor valid UTF-8, because a daemon that can round-trip `0x00 0xff` through a
/// proxy can round-trip anything.
///
/// Run against `object_store::memory::InMemory` — no network and no
/// credentials, driving the same client production drives.
#[tokio::test]
async fn test_bundle_fetch_by_hash() {
    let body: &[u8] = &[0x00, 0xff, b'a', 0x7f, 0x80];
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_snapshot(BUNDLE_HASH, body)
        .await
        .router();

    let served = send(
        &router,
        Method::GET,
        &format!("/v1/runners/me/bundles/{BUNDLE_HASH}"),
        Some(RUNNER_TOKEN),
        "",
    )
    .await;

    assert_eq!(served.status(), StatusCode::OK);
    assert_eq!(
        served
            .headers()
            .get(http::header::CONTENT_TYPE)
            .expect("a served snapshot names its type"),
        "application/x-tar"
    );
    let bytes = axum::body::to_bytes(served.into_body(), usize::MAX)
        .await
        .expect("a fixture snapshot is small and in memory");
    assert_eq!(
        bytes.as_ref(),
        body,
        "the daemon is a proxy: a byte it interprets is a byte it can get wrong"
    );

    // A well-formed hash with nothing behind it. Not an error condition in this
    // product — a skill-only bundle stores no snapshot — which is why the code
    // matters more than the status.
    let absent = send(
        &router,
        Method::GET,
        "/v1/runners/me/bundles/0000000000000000000000000000000000000000000000000000000000000000",
        Some(RUNNER_TOKEN),
        "",
    )
    .await;
    assert_eq!(absent.status(), StatusCode::NOT_FOUND);
    assert_eq!(code_of(absent).await, "UZ-BUNDLE-002");
}

/// A path segment that cannot be a digest never reaches the object store.
///
/// The check `bundles.zig` spells as an `isContentHash` guard the handler must
/// remember to call, and this spells as a type the key builder cannot be handed
/// without. The traversal case is the one it exists for: a key is rebuilt
/// server-side from a validated digest, so there is no path from request bytes
/// to a storage key at all.
#[tokio::test]
async fn test_bundle_ref_that_is_not_a_digest_is_refused() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_snapshot(BUNDLE_HASH, b"stored")
        .await
        .router();

    for refused in [
        // Uppercase: the importer writes lowercase, so folding would make two
        // spellings of one key.
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
        // Sixty-four characters, and four of them are traversal.
        "..%2F..%2Fetc%2Fpasswd00000000000000000000000000000000000000000000",
        // Right alphabet, wrong length.
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85",
    ] {
        let response = send(
            &router,
            Method::GET,
            &format!("/v1/runners/me/bundles/{refused}"),
            Some(RUNNER_TOKEN),
            "",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{refused} is not a content hash and must be refused before the store"
        );
        assert_eq!(code_of(response).await, "UZ-REQ-001");
    }
}

/// A deployment with no snapshot storage says so, rather than 404ing.
///
/// The distinction an operator acts on, and the reason `Bundles` holds its own
/// absence: a 404 would send them looking for a bundle that was never the
/// problem, when what is missing is four environment knobs.
#[tokio::test]
async fn test_bundle_fetch_without_a_store_is_unavailable() {
    // No `with_snapshot`, so the harness's default unconfigured store.
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    let refused = send(
        &router,
        Method::GET,
        &format!("/v1/runners/me/bundles/{BUNDLE_HASH}"),
        Some(RUNNER_TOKEN),
        "",
    )
    .await;

    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code_of(refused).await, "UZ-BUNDLE-005");
}

/// The bundle route is guarded like every other runner verb.
#[tokio::test]
async fn test_bundle_fetch_requires_a_runner_credential() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_snapshot(BUNDLE_HASH, b"stored")
        .await
        .router();
    let path = format!("/v1/runners/me/bundles/{BUNDLE_HASH}");

    let anonymous = send(&router, Method::GET, &path, None, "").await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(anonymous).await, "UZ-RUN-001");

    // A tenant key is refused by PLANE, before any lookup — the boundary is
    // data on the route's own row, not which middleware happened to be mounted.
    let wrong_plane = send(&router, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_eq!(wrong_plane.status(), StatusCode::UNAUTHORIZED);
}

/// The runner plane requires exactly one scope, and a runner always holds it.
///
/// Asserted against the catalogue rather than against a handler: the plane IS
/// the authorisation, so a runner-plane route that grew a second requirement
/// would be a route no runner could ever call.
#[test]
fn test_runner_scopes_satisfy_every_runner_route() {
    for route in afd_api::Route::all() {
        let meta = route.meta();
        if meta.guard != afd_api::Guard::RunnerBearer {
            continue;
        }
        for method in [Method::GET, Method::POST, Method::DELETE] {
            assert!(
                RUNNER_SCOPES.satisfies_any(meta.scopes.required(&method)),
                "{} requires a scope a runner principal does not hold",
                meta.template
            );
        }
    }
}
