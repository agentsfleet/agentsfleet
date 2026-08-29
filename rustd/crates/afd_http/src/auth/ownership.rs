//! Is this HTTP resource yours — the authorization half Zig left in handlers.
//!
//! # What this replaces
//!
//! `authorizeWorkspace` is a function each workspace handler calls at its top,
//! by hand, before touching a row. Around a hundred and sixty-five handlers
//! call it. A handler that does not is a cross-tenant read, and nothing fails
//! when somebody forgets — no test, no type, no table row says the call should
//! have been there. `cross_workspace_idor_test.zig` exists because that has
//! already happened once.
//!
//! Here it is a LAYER, mounted from [`Ownership`] which is derived from the
//! route's own template. A handler cannot forget it, because a handler is not
//! what performs it; and a route cannot opt out of it, because nothing about
//! the route says whether it wants it — the path says whether it needs it.
//!
//! # It composes with the capability gate rather than replacing it
//!
//! Two independent questions, in this order: [`super::guard::prove`] asks what
//! the caller MAY do, and this asks whose the object IS. Neither implies the
//! other — `fleet:admin` over your own workspace says nothing about mine — and
//! Milestone Invariant 1 is that both are answered on every workspace route by
//! the shared layer rather than by handler code.
//!
//! Ownership is checked SECOND, and that ordering is load-bearing: it costs a
//! datastore round trip, and a caller with no capability at all should not be
//! able to make this daemon run a statement for them.

use std::sync::Arc;

use afd_auth::principal::Principal;
use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use axum::RequestExt as _;
use axum::extract::{RawPathParams, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;
use crate::route::WORKSPACE_PARAMETER;
use crate::services::{Services, WorkspaceOwnership as _};

/// The refusal a caller reads for a workspace that is not theirs.
///
/// `Workspace access denied` under `UZ-AUTH-001`, pinned byte-for-byte to the
/// Zig handlers: a 403, not a 404, and the SAME answer for a workspace that
/// belongs to somebody else and one that does not exist. The status is a
/// parity requirement rather than a preference — a dashboard branches on it —
/// and the collapse is what keeps the endpoint from being an oracle for which
/// workspace identifiers are real.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// The refusal for a path segment that is not an identifier.
const DETAIL_MALFORMED: &str = "workspace_id must be a valid UUIDv7";

/// The workspace this request acts in, and the tenant that owns it.
///
/// Inserted by the layer and read back by handlers through
/// [`crate::auth::WorkspaceContext`]. Carrying the TENANT is the point: it was
/// resolved by the same statement that authorized the workspace, so a handler
/// that needs it does not re-read the row, and the value it uses is the one the
/// verdict was made on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owned {
    /// The workspace named in the path, parsed.
    pub workspace: Uuid7,
    /// The tenant the authorizing statement resolved.
    pub tenant: Uuid7,
}

/// Everything the ownership layer holds, resolved once when a route is mounted.
#[derive(Debug)]
pub struct Owner<D> {
    services: Arc<D>,
    template: &'static str,
}

impl<D> Owner<D> {
    /// The layer state for a route whose template is `template`.
    pub const fn new(services: Arc<D>, template: &'static str) -> Self {
        Self { services, template }
    }
}

// Hand-written for the reason [`super::guard::Gate`]'s is: `#[derive(Clone)]`
// would demand `D: Clone`, and `D` is behind an `Arc` precisely so it need not be.
impl<D> Clone for Owner<D> {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            template: self.template,
        }
    }
}

/// Refuses a workspace the caller does not own, or lets the handler run.
///
/// Mounted over exactly the routes whose [`crate::route::Ownership`] is
/// checked, so there is no "unowned" arm here to get wrong.
pub async fn own<D: Services>(
    State(owner): State<Owner<D>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(raw) = workspace_of(&mut request).await else {
        // The layer is mounted from a template that CONTAINS the parameter, so
        // a matched request always carries it. Reachable only through a router
        // assembled by hand — and the honest answer then is that this daemon
        // did not look, not that the workspace is missing.
        return refuse(
            error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_MALFORMED,
            &owner,
            "workspace_parameter_absent",
        );
    };
    let Ok(workspace) = Uuid7::parse(&raw) else {
        // Refused BEFORE the plane is asked, so the `::uuid` cast in the
        // statement can never be the thing that fails and every error from
        // below is a genuine datastore fault.
        return refuse(
            error_code::UUIDV7_INVALID_ID_SHAPE,
            DETAIL_MALFORMED,
            &owner,
            "workspace_id_malformed",
        );
    };
    // Proven by the guard layer in front of this one, which is why there is no
    // authentication arm here: a route with an ownership check is a bearer
    // route, and the two layers are mounted from the same table row.
    let Some(principal) = request.extensions().get::<Principal>().cloned() else {
        return refuse(
            error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_NOT_YOURS,
            &owner,
            "ownership_layer_without_guard",
        );
    };
    authorize(&owner, request, next, &principal, workspace).await
}

async fn authorize<D: Services>(
    owner: &Owner<D>,
    mut request: Request,
    next: Next,
    principal: &Principal,
    workspace: Uuid7,
) -> Response {
    match owner
        .services
        .workspaces()
        .authorize(principal, &workspace)
        .await
    {
        Ok(Some(tenant)) => {
            let verdict = Owned { workspace, tenant };
            request.extensions_mut().insert(verdict.clone());
            let mut response = next.run(request).await;
            // Onto the response too, for the reporting layer outside this one:
            // a request extension travels inward only, and a refusal written
            // beneath here belongs to a workspace somebody can filter on.
            response.extensions_mut().insert(verdict);
            response
        }
        // A workspace that is not this caller's, and one that does not exist,
        // are ONE answer — see [`DETAIL_NOT_YOURS`].
        Ok(None) => refuse(
            error_code::AUTH_FORBIDDEN,
            DETAIL_NOT_YOURS,
            owner,
            "workspace_not_owned",
        ),
        // A datastore that would not answer is NOT a denial. Answering "not
        // yours" for a pool timeout would tell a tenant their own workspace had
        // vanished, and a client acting on that would be acting on an outage.
        Err(error) => crate::handler::refuse(&error, "workspace_authorize_failed"),
    }
}

/// The `{workspace_id}` segment of the matched path.
///
/// Read from axum's own path parameters rather than by splitting the URI: the
/// router already matched the template and knows which segment is which, and a
/// hand-rolled split is how a route with a differently-shaped prefix ends up
/// authorizing the wrong segment.
///
/// Through [`RequestExt::extract_parts`] and not `extensions().get()`. The
/// difference is not stylistic: axum keeps matched segments in a PRIVATE
/// extension and builds [`RawPathParams`] from it in `FromRequestParts`, so
/// reaching into extensions for the public type finds nothing, every time. This
/// layer answered a 500 on every request until the first route that uses it was
/// mounted — nothing had exercised it before, because the whole workspace
/// family was tabled and unserved.
async fn workspace_of(request: &mut Request) -> Option<String> {
    let parameters = request.extract_parts::<RawPathParams>().await.ok()?;
    let name = WORKSPACE_PARAMETER.trim_matches(['{', '}']);
    parameters
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
}

/// Writes a refusal, and logs it against the same request id the caller sees.
fn refuse<D>(
    code: ErrorCode,
    detail: &'static str,
    owner: &Owner<D>,
    event: &'static str,
) -> Response {
    let request_id = RequestId::mint();
    // Hoisted out of the macro: `tracing`'s `log` bridge compiles a second copy
    // of every field expression, and llvm-cov scores the copy that never runs.
    let code_field = code.as_str();
    let request_id_field = request_id.as_str();
    let template = owner.template;
    // `debug`, not `warn`: a refused cross-tenant read is the boundary working.
    // At `warn`, anybody probing identifiers would be the loudest thing in an
    // operator's log. The template rather than the path, because a real path
    // carries the identifier that was probed.
    tracing::debug!(
        error_code = code_field,
        request_id = request_id_field,
        route = template,
        event,
    );
    ProblemResponse::new(code, detail, request_id).into_response()
}

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
        std::future::ready(
            parts
                .extensions
                .get::<Owned>()
                .cloned()
                .map(Self)
                .ok_or_else(|| {
                    let request_id = RequestId::mint();
                    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                    let request_id_field = request_id.as_str();
                    // `error`: a handler asking whose workspace this is, mounted
                    // on a route whose template carries no workspace, is a
                    // routing table and a router disagreeing. No client
                    // behaviour causes it and no retry fixes it.
                    tracing::error!(
                        error_code = code,
                        request_id = request_id_field,
                        event = "workspace_context_absent",
                        "a workspace handler ran with no ownership verdict — its layer is not mounted"
                    );
                    ProblemResponse::new(
                        error_code::INTERNAL_OPERATION_FAILED,
                        DETAIL_NOT_YOURS,
                        request_id,
                    )
                    .into_response()
                }),
        )
    }
}

/// The caller themselves, for the one surface that has to ask again.
///
/// Every other verb is authorized once, by the layer, and is finished before
/// the answer could go stale. A live stream is open for as long as somebody
/// has a tab, so its membership check has to RUN AGAIN on a tick — and running
/// again needs the principal, not just the verdict the layer reached. This is
/// the only reason it is extractable at all.
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
                    let request_id = RequestId::mint();
                    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                    let request_id_field = request_id.as_str();
                    // `error`, for the reason the sibling above is: a handler
                    // naming the caller, mounted on a route with no guard layer, is
                    // the routing table and the router disagreeing.
                    tracing::error!(
                        error_code = code,
                        request_id = request_id_field,
                        event = "principal_absent",
                        "a handler asked who the caller is with no guard in front of it"
                    );
                    ProblemResponse::new(
                        error_code::INTERNAL_OPERATION_FAILED,
                        DETAIL_NOT_YOURS,
                        request_id,
                    )
                    .into_response()
                }),
        )
    }
}

#[cfg(test)]
mod tests;
