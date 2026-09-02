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
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme};

use crate::Route;
use crate::route::{Guard, Verb};

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
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(BEARER_DESCRIPTION))
                    .build(),
            ),
        );
    require_the_credential_each_route_guards(&mut document);
    document
}

/// Publishes, per operation, the credential the router already demands.
///
/// # Why this is derived and not annotated
///
/// The alternative is `security(("BearerAuth" = []))` on all 102
/// `#[utoipa::path]` blocks, and it rots the day someone adds the 103rd: a
/// forgotten clause is not a compile error, and under `OpenAPI` 3.1 an operation
/// with no `security` — in a document with no root `security` — requires NO
/// authentication. The omission publishes the route as open rather than
/// leaving it undescribed, so the failure is silent and points the wrong way.
///
/// `RouteMeta::guard` already decides this before any handler runs, which is
/// the same argument the coverage gate makes about paths: two declarations of
/// one fact cannot be kept in agreement by hand, so only one of them is
/// written down.
///
/// # Why an open route gets an empty requirement rather than none
///
/// `security: []` is how an operation says "no credential", and saying it is
/// not the same as omitting it. A webhook authenticated by its payload
/// signature is deliberately open; a bearer route that says nothing is a bug.
/// Emitting the empty list keeps those two apart in the published document.
fn require_the_credential_each_route_guards(document: &mut utoipa::openapi::OpenApi) {
    for route in Route::all() {
        let meta = route.meta();
        let required = if meta.guard == Guard::Open {
            Vec::new()
        } else {
            vec![SecurityRequirement::new(
                BEARER_SCHEME,
                Vec::<String>::new(),
            )]
        };
        let Some(item) = document.paths.paths.get_mut(meta.template) else {
            continue; // the coverage gate owns a route the document is missing
        };
        for verb in route.verbs() {
            let operation = match verb {
                Verb::Get => item.get.as_mut(),
                Verb::Post => item.post.as_mut(),
                Verb::Put => item.put.as_mut(),
                Verb::Patch => item.patch.as_mut(),
                Verb::Delete => item.delete.as_mut(),
            };
            if let Some(operation) = operation {
                operation.security = Some(required.clone());
            }
        }
    }
}
