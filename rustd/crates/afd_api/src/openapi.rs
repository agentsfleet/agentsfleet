//! The merged `OpenAPI` document: every plane's collector, and this crate's probes.
//!
//! # What merging is, and what it is not
//!
//! Each plane crate owns the routes it serves and publishes them through its
//! own `document()`. This module puts the five documents together and adds the
//! things that belong to the daemon rather than to any one plane — the title,
//! the servers, the security scheme every bearer route names.
//!
//! It does NOT decide what is served. That is `mount.rs`'s total match, and the
//! coverage gate is what holds this document to it: a route mounted there and
//! missing here fails, and so does the reverse.

use utoipa::OpenApi as _;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

/// The scheme name every bearer-guarded operation refers to.
const BEARER_SCHEME: &str = "BearerAuth";

/// How a caller obtains the bearer this API is read with.
const BEARER_DESCRIPTION: &str =
    "Obtain a token via the CLI auth flow (POST /v1/auth/sessions) or GitHub OAuth";

/// The published base URL.
const PRODUCTION_URL: &str = "https://api.agentsfleet.net";

/// What that base URL is.
const PRODUCTION_DESCRIPTION: &str = "Production";

/// The composition root's own routes: the two probes, which belong to no plane.
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "agentsfleet Control Plane API",
        version = "1.0.0",
        description = "API for managing workspaces, fleets, triggers, and runs.",
        contact(name = "agentsfleet", url = "https://agentsfleet.net"),
    ),
    paths(crate::router::probes::healthz, crate::router::probes::readyz,)
)]
struct Root;

/// Everything this daemon serves, as one `OpenAPI` document.
///
/// The order the planes merge in does not matter: paths are keyed by template,
/// and no two planes serve the same one. Two ROUTES may share a template — the
/// connector callback pair, the session poll and delete — but they differ by
/// method and land as separate operations under one path item.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    let mut document = Root::openapi();
    document.merge(afd_api_tenant::openapi::document());
    document.merge(afd_api_runner::openapi::document());
    document.merge(afd_api_operator::openapi::document());
    document.merge(afd_api_ingress::openapi::document());
    document.servers = Some(vec![
        utoipa::openapi::ServerBuilder::new()
            .url(PRODUCTION_URL)
            .description(Some(PRODUCTION_DESCRIPTION))
            .build(),
    ]);
    if let Some(components) = document.components.as_mut() {
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(BEARER_DESCRIPTION))
                    .build(),
            ),
        );
    }
    document
}
