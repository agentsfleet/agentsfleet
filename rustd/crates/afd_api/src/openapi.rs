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

use afd_http::openapi::problem::ProblemBody;
use http::StatusCode;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme};
use utoipa::openapi::{Content, Ref, RefOr};
use utoipa::{OpenApi as _, ToSchema as _};

use crate::Route;
use crate::envelope::CONTENT_TYPE_PROBLEM_JSON;
use crate::route::{Guard, Verb};

/// The scheme name every tenant-plane operation refers to.
const BEARER_SCHEME: &str = "BearerAuth";

/// How a caller obtains the bearer the tenant plane is read with.
const BEARER_DESCRIPTION: &str =
    "Obtain a token via the CLI auth flow (POST /v1/auth/sessions) or GitHub OAuth";

/// The scheme name every runner-plane operation refers to.
///
/// A second scheme rather than one shared with the tenant plane: a runner's
/// `agt_r` token is an opaque credential minted at enrolment, not a JWT a
/// person signs in for, and a document that described it under the tenant
/// scheme told a runner author to go through the CLI auth flow.
const RUNNER_SCHEME: &str = "RunnerBearerAuth";

/// How a runner obtains its bearer.
const RUNNER_DESCRIPTION: &str =
    "The opaque agt_r token minted when the runner enrols (POST /v1/runners)";

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
    paths(crate::router::probes::healthz, crate::router::probes::readyz,),
    components(schemas(ProblemBody))
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
    let components = document.components.get_or_insert_with(Default::default);
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
    components.add_security_scheme(
        RUNNER_SCHEME,
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("agt_r")
                .description(Some(RUNNER_DESCRIPTION))
                .build(),
        ),
    );
    require_the_credential_each_route_guards(&mut document);
    describe_every_refusal_as_a_problem(&mut document);
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
///
/// # Why the split is bearer-or-not, and not open-or-not
///
/// Four guards are not `Open` and still take no bearer: the HMAC, signature
/// and Svix guards prove a delivery by its payload, and the authenticator
/// layer treats them exactly as it treats `Open`. Publishing them under the
/// bearer scheme told every integrator to send a JWT that no handler reads.
fn require_the_credential_each_route_guards(document: &mut utoipa::openapi::OpenApi) {
    for route in Route::all() {
        let meta = route.meta();
        let required = match meta.guard {
            Guard::Bearer => vec![SecurityRequirement::new(
                BEARER_SCHEME,
                Vec::<String>::new(),
            )],
            Guard::RunnerBearer => vec![SecurityRequirement::new(
                RUNNER_SCHEME,
                Vec::<String>::new(),
            )],
            Guard::Open
            | Guard::WebhookHmac
            | Guard::WebhookSignature
            | Guard::Svix
            | Guard::PayloadSigned => Vec::new(),
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

/// Publishes, on every refusal, the body the envelope writer sends.
///
/// # Why this is derived and not annotated
///
/// The same argument as the credential: every 4xx and 5xx this daemon writes
/// goes through one writer with one shape, and saying so at each of the
/// several hundred `responses(...)` clauses is several hundred chances to
/// forget. Before this pass, one refusal in the document carried a body and
/// the rest carried a sentence, so every generated client typed a refusal as
/// nothing and the `error_code` a caller switches on was unreachable.
///
/// # Why a refusal that already describes a body is left alone
///
/// `/readyz` answers 503 with its readiness report, not with a refusal: it
/// never touches the envelope writer, and its body is the one the probe
/// annotation names. A response that already says what it carries is telling
/// the truth about a different shape, and overwriting it would publish a
/// problem body the probe never sends.
fn describe_every_refusal_as_a_problem(document: &mut utoipa::openapi::OpenApi) {
    let body = Ref::from_schema_name(ProblemBody::name());
    for item in document.paths.paths.values_mut() {
        // Every operation a `PathItem` can carry, not the five this daemon
        // happens to mount today: a `head` added later would otherwise publish
        // its refusals bodyless, and nothing would say so.
        let operations = [
            &mut item.get,
            &mut item.post,
            &mut item.put,
            &mut item.patch,
            &mut item.delete,
            &mut item.head,
            &mut item.options,
            &mut item.trace,
        ];
        for operation in operations.into_iter().flatten() {
            for (code, response) in &mut operation.responses.responses {
                if !is_a_refusal(code) {
                    continue;
                }
                let RefOr::T(response) = response else {
                    continue; // a shared response object describes itself
                };
                if !response.content.is_empty() {
                    continue;
                }
                response.content.insert(
                    CONTENT_TYPE_PROBLEM_JSON.to_owned(),
                    Content::new(Some(body.clone())),
                );
            }
        }
    }
}

/// Whether a response key names a client or server error.
///
/// Three spellings, because `OpenAPI` allows three. A status is the common one;
/// `4XX` and `5XX` are the range keys, which a plane may reach for; and
/// `default` is the catch-all, which in a document whose every named response
/// is a success can only be describing a failure. The success and redirect
/// statuses are not refusals — the envelope writer never answers them — and
/// the contract test reads this same function so the two cannot disagree.
#[must_use]
pub fn is_a_refusal(code: &str) -> bool {
    matches!(
        code,
        RANGE_CLIENT_ERROR | RANGE_SERVER_ERROR | DEFAULT_RESPONSE
    ) || code
        .parse::<StatusCode>()
        .is_ok_and(|status| status.is_client_error() || status.is_server_error())
}

/// The `OpenAPI` range key covering every client error.
const RANGE_CLIENT_ERROR: &str = "4XX";

/// The `OpenAPI` range key covering every server error.
const RANGE_SERVER_ERROR: &str = "5XX";

/// The `OpenAPI` catch-all response key.
const DEFAULT_RESPONSE: &str = "default";
