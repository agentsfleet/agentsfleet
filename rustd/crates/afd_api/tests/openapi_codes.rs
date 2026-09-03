//! What an operation publishes for a refusal is what the route table decides.
//!
//! # Why this is not a comparison against the error registry
//!
//! The obvious reading of "documented codes match the handler's refusals" is a
//! per-handler enumeration of every `UZ-*` it can raise, and that is not
//! statically derivable: a handler's refusals come from whichever plane error
//! its service layer returns, through `Refusable::status()`, several calls
//! down. Deriving that would mean an analysis the compiler does not offer.
//!
//! What IS decided in one place, and decided BEFORE any handler runs, is the
//! authenticator's half. `RouteMeta` says whether a route is guarded and which
//! capability it needs; the layers in front of the handler answer 401 and 403
//! from exactly those two facts. So three codes are not a matter of taste:
//!
//!   * a route whose guard is not `Open` can always refuse an absent or
//!     unacceptable credential — 401, `AUTH_UNAUTHORIZED`
//!   * a route with a non-empty scope rung can always refuse a credential that
//!     lacks it — 403, `AUTH_FORBIDDEN`, from `require_scope`
//!   * anything that reaches a plane can fail — 500
//!   * anything that reaches a plane can find it unreachable — the datastore
//!     behind it, or the credential directory in front of it — 503
//!   * anything the admission ceiling meters can be shed — 429,
//!     `API_BACKPRESSURE`, from `admission::is_metered`
//!   * anything whose path names a workspace is checked by the ownership
//!     layer before the handler runs, which refuses a malformed id with 400
//!     and somebody else's workspace with 403 — from `RouteMeta::ownership`
//!   * anything that reads a body is bounded by the body limit — 413
//!
//! The probes are the exception to that last one, and not a grudging one: they
//! answer from process state without touching a service, so `/healthz` can only
//! say 200 and `/readyz` only 200 or 503. Documenting a 500 they cannot produce
//! would be the same defect in the other direction. `RouteClass::Ops` is the
//! table's own name for that class, so it is what this reads.
//!
//! An operation that omits one of those documents a refusal a caller will meet
//! and was not told about, which is the defect the committed contract had at
//! scale: most of its operations listed a success and `default` and nothing in
//! between.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

use std::collections::{BTreeMap, BTreeSet};

use afd_api::Route;
use afd_api::route::{Guard, RouteClass};

/// Refused for want of a credential.
const UNAUTHORIZED: u16 = 401;

/// Refused for want of a capability.
const FORBIDDEN: u16 = 403;

/// The daemon could not answer.
const INTERNAL: u16 = 500;

/// The daemon could not reach what the answer needs.
const UNAVAILABLE: u16 = 503;

/// The instance is at its ceiling and shed the request.
const TOO_MANY_REQUESTS: u16 = 429;

/// The request could not be read: a malformed identifier in the path.
const BAD_REQUEST: u16 = 400;

/// The body is over the ceiling the extractor enforces.
const PAYLOAD_TOO_LARGE: u16 = 413;

/// One operation, keyed as both sides spell it.
type Operation = (String, String);

/// The scheme `document()` registers for the tenant plane.
const BEARER_SCHEME: &str = "BearerAuth";

/// The scheme `document()` registers for the runner plane.
const RUNNER_SCHEME: &str = "RunnerBearerAuth";

/// The scheme a guard publishes, or `None` for a route that takes no bearer.
fn scheme_of(guard: Guard) -> Option<&'static str> {
    match guard {
        Guard::Bearer => Some(BEARER_SCHEME),
        Guard::RunnerBearer => Some(RUNNER_SCHEME),
        Guard::Open
        | Guard::WebhookHmac
        | Guard::WebhookSignature
        | Guard::Svix
        | Guard::PayloadSigned => None,
    }
}

/// The codes each operation publishes, read out of the generated document.
fn published() -> BTreeMap<Operation, BTreeSet<u16>> {
    let mut out = BTreeMap::new();
    for (path, item) in afd_api::openapi::document().paths.paths {
        for (operation, method) in [
            (&item.get, "GET"),
            (&item.post, "POST"),
            (&item.put, "PUT"),
            (&item.patch, "PATCH"),
            (&item.delete, "DELETE"),
        ] {
            if let Some(operation) = operation.as_ref() {
                let codes = operation
                    .responses
                    .responses
                    .keys()
                    .filter_map(|code| code.parse::<u16>().ok())
                    .collect();
                out.insert((path.clone(), method.to_owned()), codes);
            }
        }
    }
    out
}

/// The operations that read a request body, read out of the generated document.
///
/// Every body extractor sits under a body limit, so a body-taking operation can
/// always answer 413; the document is the one place that says which ones take
/// a body, since the annotation is what declares it.
fn reads_a_body() -> BTreeSet<Operation> {
    let document =
        serde_json::to_value(afd_api::openapi::document()).expect("the document serializes");
    let mut out = BTreeSet::new();
    let paths = document.get("paths").and_then(serde_json::Value::as_object);
    for (path, item) in paths.into_iter().flatten() {
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            if operation.get("requestBody").is_some() {
                out.insert((path.clone(), method.to_uppercase()));
            }
        }
    }
    out
}

/// The operations that name the bearer scheme, read out of the generated document.
///
/// `SecurityRequirement` keeps its one field private, so the requirement cannot
/// be inspected through the builder's types the way the codes above are. The
/// serialized form is the more honest subject anyway: what a caller reads is
/// `public/openapi.json`, not a `BTreeMap`.
///
/// An EMPTY `security` array is deliberately not a match. It is how an
/// operation says "no credential", which is right for an open route and a lie
/// for a guarded one.
fn credentialed() -> BTreeMap<Operation, &'static str> {
    let document =
        serde_json::to_value(afd_api::openapi::document()).expect("the document serializes");
    let scheme_named = |security: Option<&serde_json::Value>| -> Option<&'static str> {
        let requirements = security.and_then(serde_json::Value::as_array)?;
        [BEARER_SCHEME, RUNNER_SCHEME]
            .into_iter()
            .find(|scheme| requirements.iter().any(|one| one.get(scheme).is_some()))
    };
    let mut out = BTreeMap::new();
    if let Some(scheme) = scheme_named(document.get("security")) {
        for route in Route::all() {
            let template = route.meta().template;
            for verb in route.verbs() {
                out.insert((template.to_owned(), verb.method().to_string()), scheme);
            }
        }
        return out;
    }
    let paths = document.get("paths").and_then(serde_json::Value::as_object);
    for (path, item) in paths.into_iter().flatten() {
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            if let Some(scheme) = scheme_named(operation.get("security")) {
                out.insert((path.clone(), method.to_uppercase()), scheme);
            }
        }
    }
    out
}

/// A bearer route publishes the credential that gets a caller past its guard,
/// and a payload-signed route publishes that it takes none.
///
/// The same `RouteMeta::guard` that decides the 401 above decides this. Under
/// `OpenAPI` 3.1 an operation with no `security`, in a document with no root
/// `security`, requires NO authentication — a positive claim, not an absence.
/// A generated client omits the `Authorization` header and a spec-driven
/// gateway lets the call through, so a guarded route that says nothing here is
/// published as open. The other direction is a lie too: a webhook the router
/// verifies by signature that names the bearer scheme tells every integrator
/// to send a JWT no handler reads.
#[test]
fn test_every_guarded_operation_names_its_credential() {
    let credentialed = credentialed();
    let mut naked = Vec::new();

    for route in Route::all() {
        let meta = route.meta();
        for verb in route.verbs() {
            let key = (meta.template.to_owned(), verb.method().to_string());
            if credentialed.get(&key).copied() != scheme_of(meta.guard) {
                naked.push(format!(
                    "{} {} (guard {:?})",
                    verb.method(),
                    meta.template,
                    meta.guard
                ));
            }
        }
    }

    assert!(
        naked.is_empty(),
        "an operation's published credential disagrees with its guard: a bearer \
         route that names none is published as open, and a payload-signed \
         route that names a bearer demands a token nobody reads, and a runner \
         route under the tenant scheme sends its author through the CLI sign-in \
         ({} of them):\n  {}",
        naked.len(),
        naked.join("\n  "),
    );
}

/// Every operation publishes the refusals its own route metadata implies.
#[test]
fn test_documented_codes_match_refusals() {
    let published = published();
    let reads_a_body = reads_a_body();
    let mut missing = Vec::new();

    for route in Route::all() {
        let meta = route.meta();
        for verb in route.verbs() {
            let method = verb.method();
            let mut required = Vec::new();
            if meta.class != RouteClass::Ops {
                required.push(INTERNAL);
                required.push(UNAVAILABLE);
            }
            if afd_http::admission::is_metered(meta.class) {
                required.push(TOO_MANY_REQUESTS);
            }
            if meta.ownership.is_checked() {
                required.push(BAD_REQUEST);
                required.push(FORBIDDEN);
            }
            if reads_a_body.contains(&(meta.template.to_owned(), method.to_string())) {
                required.push(PAYLOAD_TOO_LARGE);
            }
            if meta.guard != Guard::Open {
                required.push(UNAUTHORIZED);
            }
            if !meta.scopes.required(&method).is_empty() {
                required.push(FORBIDDEN);
            }
            let key = (meta.template.to_owned(), method.to_string());
            let Some(codes) = published.get(&key) else {
                continue; // the coverage gate owns this direction
            };
            for code in required {
                if !codes.contains(&code) {
                    missing.push(format!(
                        "{method} {} does not publish {code}",
                        meta.template
                    ));
                }
            }
        }
    }

    assert!(
        missing.is_empty(),
        "an operation omits a refusal its route metadata guarantees a caller \
         can meet:\n  {}",
        missing.join("\n  ")
    );
}
