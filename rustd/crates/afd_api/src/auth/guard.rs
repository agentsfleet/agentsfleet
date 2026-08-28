//! The layer that stands between a route and its handler.
//!
//! Two checks, in this order and no other: prove the credential, then check the
//! capability it resolved to. Both refuse before the handler runs, so a handler
//! never sees a request it should not have — which is what lets every handler
//! in this crate take an identity as an ARGUMENT rather than re-deriving one.
//!
//! # Why the route's facts are captured at mount time
//!
//! `route_table.zig` switches on the route inside `dispatch`, on every request,
//! to find out which middleware chain to run. Nothing about that switch can
//! change between requests: a route's guard and scope rung are constants in the
//! table. Here [`Gate`] captures them while the router is being built, so the
//! request path reads a `Copy` struct it already holds instead of re-deciding
//! what it is.
//!
//! # What is deliberately NOT memoised
//!
//! The verdict. `docs/AUTH.md` makes the row read per request the revocation
//! channel for the runner plane — there is no cached decision to invalidate,
//! because there is no cache. This layer holds no state beyond the route's own
//! constants, and `test_revocation_immediate` is what keeps it that way.

use std::sync::Arc;

use afd_auth::gate::require_scope;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use http::header;

use crate::auth::{Authenticator as _, plane_of};
use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;
use crate::route::RouteMeta;
use crate::services::Services;

/// Everything one guarded route needs, resolved once when it is mounted.
///
/// Cheap to clone — an `Arc` and a `Copy` — which it must be, because axum
/// clones the layer's state per request.
#[derive(Debug)]
pub struct Gate<D> {
    services: Arc<D>,
    meta: RouteMeta,
}

impl<D> Gate<D> {
    /// The gate for a route whose facts are `meta`.
    pub const fn new(services: Arc<D>, meta: RouteMeta) -> Self {
        Self { services, meta }
    }
}

// Hand-written rather than derived: `#[derive(Clone)]` would demand `D: Clone`,
// and `D` is behind an `Arc` precisely so it does not have to be.
impl<D> Clone for Gate<D> {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            meta: self.meta,
        }
    }
}

/// Proves the credential and the capability, or refuses.
///
/// Mounted with [`axum::middleware::from_fn_with_state`] over exactly the
/// routes whose [`crate::route::Guard`] names a plane. A route whose guard
/// names none never reaches this function, which is why there is no
/// "unguarded" arm here to get wrong.
pub async fn prove<D: Services>(
    State(gate): State<Gate<D>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(plane) = plane_of(gate.meta.guard) else {
        // Unreachable through the router, which mounts this layer only where
        // `plane_of` answered `Some`. Written as a pass-through rather than a
        // refusal so that a future guard added to the table cannot be silently
        // closed by a layer that was never meant to judge it.
        return next.run(request).await;
    };

    let header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let principal = match gate
        .services
        .authenticator()
        .authenticate(plane, header)
        .await
    {
        Ok(principal) => principal,
        Err(refusal) => {
            // A non-ASCII header value lands here too, as an unreadable
            // credential rather than as a distinct failure — `to_str` failing
            // means the bytes cannot be a bearer token, which is the same
            // answer as a bearer token that proves nothing.
            return refuse(
                refusal.code(),
                refusal.detail(),
                &gate,
                "credential refused",
            );
        }
    };

    let required = gate.meta.scopes.required(request.method());
    if let Err(denied) = require_scope(&principal, required) {
        return refuse(
            denied.code(),
            // The whole requirement, because the gate is any-of: naming one
            // missing scope would tell a caller to obtain that one when any of
            // the others would also have let them through.
            denied.to_string(),
            &gate,
            "capability refused",
        );
    }

    // The handler reads this back through a typed extractor. An extension
    // rather than a parameter because axum has no other way to hand a value
    // from a layer to a handler — and the extractor is what keeps that
    // stringly-typed lookup out of every handler body.
    request.extensions_mut().insert(principal.clone());
    let mut response = next.run(request).await;
    // And onto the RESPONSE, for the reporting layer outside this one. A
    // request extension travels inward only, so a refusal written beneath here
    // would otherwise be attributed to nobody — which is the difference between
    // "a person hit a wall" and "something did".
    response.extensions_mut().insert(principal);
    response
}

/// Writes a refusal, and logs it against the same request id the caller sees.
///
/// One function for both checks, so an authentication refusal and a capability
/// refusal cannot end up shaped differently — which is the drift twelve
/// hand-written `ctx.fail` call sites across four Zig middleware files exist to
/// demonstrate.
fn refuse<D>(
    code: afd_core::error_code::ErrorCode,
    detail: impl Into<String>,
    gate: &Gate<D>,
    event: &'static str,
) -> Response {
    let request_id = RequestId::mint();
    // Hoisted out of the macro: `tracing`'s `log` bridge compiles a second copy
    // of every field expression, and llvm-cov scores the copy that never runs.
    let code_field = code.as_str();
    let request_id_field = request_id.as_str();
    let template = gate.meta.template;
    // `debug`, not `warn`: a refused credential is the surface behaving
    // correctly, and at `warn` a scanner probing the API would be the loudest
    // thing in an operator's log. The template rather than the path, because a
    // real path carries workspace and fleet identifiers.
    tracing::debug!(
        error_code = code_field,
        request_id = request_id_field,
        route = template,
        event,
    );
    ProblemResponse::new(code, detail, request_id).into_response()
}
