//! `GET /v1/runners/me/bundles/{content_hash}` — the fleet's support files.
//!
//! # The daemon is the proxy because the runner holds no keys
//!
//! A runner's only credential is its own `agt_r` token; it has no object-store
//! credentials and is never given any. So it learns a bundle's content hash from
//! its lease payload and asks here, and the daemon — which does hold the keys —
//! serves the immutable canonical tar.
//!
//! # There is no ownership check, and that is the design
//!
//! `bundles.zig` opens by refusing a principal with no `runner_id`, which the
//! [`RunnerIdentity`] extractor has already done by the time this function
//! exists — the route's guard is `RunnerBearer` and a tenant credential never
//! reaches it. What neither implementation does is check that THIS runner's
//! lease named THIS bundle, and the reason is that the snapshot is
//! content-addressed by SHA-256 and holds no secrets: resolved secret values
//! ride the lease's `secret_delivery`, never the archive. An authenticated
//! runner presenting an unguessable 256-bit digest is the access boundary, and
//! the digest is validated into a type before it can reach a storage key.
//!
//! # Why the body is not JSON
//!
//! It is a tar, and the only handler on this plane that answers bytes. Nothing
//! about it is a wire type, so nothing about it is fixture-pinned.

use std::sync::Arc;

use afd_fleet::bundle::ContentHash;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::header::{self, HeaderValue};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::Services;

/// The scoped event a failed fetch is logged under.
const EVENT: &str = "runner_bundle_fetch_failed";

/// What a canonical snapshot is, and it is not negotiable — `importer.zig`
/// writes one shape and this serves it.
const TAR: &str = "application/x-tar";

/// [`TAR`], as the response header carries it.
const CONTENT_TYPE_TAR: HeaderValue = HeaderValue::from_static(TAR);

/// Serves one bundle's canonical tar by content hash.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/runners/me/bundles/{content_hash}",
    tag = afd_http::openapi::tag::FLEET_BUNDLES,
    operation_id = "runner_fetch_bundle",
    summary = "Fetch a fleet's support-file bundle",
    description = concat!(
        "The canonical tar a runner materialises support files from, ",
        "addressed by content hash. The daemon proxies it because the runner ",
        "holds no object-store keys. A hash is not a name anybody can guess, ",
        "so the snapshot is not scoped to a runner, a fleet or a tenant. ",
    ),
    params(
        afd_http::openapi::path::Bundle,
    ),
    responses(
        (status = 200, description = "The canonical tar, byte for byte as it was imported", body = afd_http::openapi::body::Binary, content_type = TAR),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    // Bound and immediately dropped. The extractor is what PROVES a runner is
    // asking, and this verb needs nothing else from the identity — a snapshot
    // is not scoped to a runner, a fleet or a tenant, because a content hash is
    // not a name anybody can guess.
    RunnerIdentity(_runner): RunnerIdentity,
    Path(content_hash): Path<String>,
) -> Response {
    let hash = match ContentHash::parse(&content_hash) {
        Ok(hash) => hash,
        // Refused here rather than in the store, and it is the same shape the
        // memory verb's `fleet_id` takes: a segment that cannot be a digest can
        // never name a snapshot, so it never becomes a storage key at all.
        Err(error) => return refuse(&error, EVENT),
    };

    match services.bundles().fetch(hash).await {
        Ok(snapshot) => ([(header::CONTENT_TYPE, CONTENT_TYPE_TAR)], snapshot).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
