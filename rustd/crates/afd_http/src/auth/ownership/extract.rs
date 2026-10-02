//! The two extractors a handler declares to read what the guard and the
//! ownership layer already decided. Neither re-checks anything: each reads a
//! request extension a layer inserted, and fails closed when that layer is not
//! mounted, because a handler running without it is a routing fault.

use afd_auth::principal::Principal;
use afd_core::error_code;
use axum::response::{IntoResponse as _, Response};

use super::{DETAIL_NOT_YOURS, Owned};
use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The workspace a handler is acting in, as a parameter it declares.
///
/// A handler that names it in its signature is a handler that ran behind the
/// ownership layer — and one that does not name it still ran behind the layer,
/// because the layer is mounted from the route rather than from the signature.
/// What the extractor adds is access to the TENANT the verdict resolved,
/// without a second read of the row.
#[derive(Debug, Clone)]
pub struct WorkspaceContext(pub Owned);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for WorkspaceContext {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(parts.extensions.get::<Owned>().cloned().map(Self).ok_or_else(|| {
            // `error`: a handler asking whose workspace this is, mounted on a
            // route whose template carries no workspace, is a routing table
            // and a router disagreeing. No client behaviour causes it and no
            // retry fixes it.
            layer_absent(
                "workspace_context_absent",
                "a workspace handler ran with no ownership verdict — its layer is not mounted",
            )
        }))
    }
}

/// The caller themselves, for the two surfaces the layer cannot serve.
///
/// Every other verb is authorized once, by the layer mounted from the route's
/// own template, and is finished before the answer could go stale. Two are not,
/// for different reasons, and both need the principal rather than the verdict
/// the layer reached:
///
/// - A live stream is open for as long as somebody has a tab, so its membership
///   check has to RUN AGAIN on a tick.
/// - The connector completion names no workspace in its PATH — the workspace is
///   inside the signed state, unreadable until the signature has been checked —
///   so `Ownership::of` mounts nothing and the check happens in the handler, at
///   the one point in the order where it is both possible and still ahead of
///   the nonce spend. See [`crate::handler::connector::callback`].
///
/// Those are the only reasons it is extractable at all. A third caller is a
/// route that should have declared its workspace in its template.
#[derive(Debug, Clone)]
pub struct Acting(pub Principal);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Acting {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<Principal>()
                .cloned()
                .map(Self)
                .ok_or_else(|| {
                    // `error`, for the reason the sibling above is: a handler
                    // naming the caller, mounted on a route with no guard
                    // layer, is the routing table and the router disagreeing.
                    layer_absent(
                        "principal_absent",
                        "a handler asked who the caller is with no guard in front of it",
                    )
                }),
        )
    }
}

/// The refusal for a handler whose layer is not mounted, logged as the routing
/// fault it is: `event` names which layer, `message` what went missing.
fn layer_absent(event: &'static str, message: &'static str) -> Response {
    let request_id = RequestId::mint();
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let request_id_field = request_id.as_str();
    tracing::error!(
        error_code = code,
        request_id = request_id_field,
        event,
        message
    );
    ProblemResponse::new(
        error_code::INTERNAL_OPERATION_FAILED,
        DETAIL_NOT_YOURS,
        request_id,
    )
    .into_response()
}
