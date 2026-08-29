//! Runner bundle routing, validation, storage, and authorization proofs.

use super::*;

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
