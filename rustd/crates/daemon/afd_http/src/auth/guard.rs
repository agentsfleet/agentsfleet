//! One middleware proves a principal through its selected strategy, then checks scope.

use afd_auth::gate::require_scope;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse as _, Response},
};

use super::RequestAuthentication;
use crate::{envelope::ProblemResponse, request_id::RequestId, route::RouteMeta};

/// Authentication strategy and route policy, selected together when the route is mounted.
#[derive(Debug, Clone)]
pub struct Gate<A> {
    authentication: A,
    meta: RouteMeta,
}

impl<A> Gate<A> {
    /// Bind a concrete strategy to the route that declares its required scopes.
    pub const fn new(authentication: A, meta: RouteMeta) -> Self {
        Self {
            authentication,
            meta,
        }
    }
}

/// Authenticate through the route's strategy and apply the common scope check.
pub async fn prove<A: RequestAuthentication>(
    State(gate): State<Gate<A>>,
    request: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = request.into_parts();
    let principal = match gate
        .authentication
        .authenticate(&mut parts, gate.meta)
        .await
    {
        Ok(Some(principal)) => principal,
        Err(refusal) => return refusal.into_response(),
        Ok(None) => return next.run(Request::from_parts(parts, body)).await,
    };
    let required = gate.meta.scopes.required(&parts.method);
    if let Err(denied) = require_scope(&principal, required) {
        return reject(
            gate.meta,
            denied.code(),
            denied.to_string(),
            "capability_refused",
        );
    }
    parts.extensions.insert(principal.clone());
    let mut response = next.run(Request::from_parts(parts, body)).await;
    response.extensions_mut().insert(principal);
    response
}

pub(super) fn reject(
    meta: RouteMeta,
    code: af_core::error_code::ErrorCode,
    detail: impl Into<String>,
    event: &'static str,
) -> Response {
    let request_id = RequestId::mint();
    let error_code = code.as_str();
    let request_id_field = request_id.as_str();
    let route = meta.template;
    tracing::debug!(error_code, request_id = request_id_field, route, event);
    ProblemResponse::new(code, detail, request_id).into_response()
}
