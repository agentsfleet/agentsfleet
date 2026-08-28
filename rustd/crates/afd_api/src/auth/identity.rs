//! The proven identity, as a parameter a handler declares.
//!
//! # Why an extractor and not `Extension<Principal>`
//!
//! Every runner-plane handler needs the same two things: that a principal was
//! proven at all, and that it is a RUNNER. `Extension<Principal>` gives a
//! handler the first and leaves the second to a `match` in each body — ten
//! verbs, ten copies of the same narrowing, and each one free to answer a
//! different code when it fails.
//!
//! [`RunnerIdentity`] is that narrowing, once. A handler that names it in its
//! signature is a handler that cannot run for a tenant caller, and the arm
//! where it would have has nowhere left to live.
//!
//! # The failure arm, and what it actually means
//!
//! Neither branch below is reachable through this crate's router, and that is
//! stated rather than assumed: [`crate::router::build`] mounts a runner route
//! only through a helper that applies the guard layer with the SAME
//! [`RouteMeta`] the route declares, so a runner handler with no runner
//! principal in front of it cannot be assembled.
//!
//! What the arm exists for is the day somebody assembles one anyway — a
//! handler mounted directly in a test, or a route table edited without its
//! router. The honest answer then is 500: the daemon did not look at the
//! caller's credential, so telling them their credential is bad would be a
//! wiring bug wearing an authentication error's clothes, which is the exact
//! failure `core_api`'s registry-by-equality produces in production.
//!
//! [`RouteMeta`]: crate::route::RouteMeta

use afd_auth::principal::{Principal, Runner};
use afd_core::error_code;
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse as _, Response};
use http::request::Parts;

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The detail a mis-mounted handler answers with.
///
/// Deliberately says nothing about credentials: the caller's was never judged.
const DETAIL_UNPROVEN: &str = "runner identity required";

/// A runner speaking for itself, proven by the layer in front of the handler.
#[derive(Debug, Clone)]
pub struct RunnerIdentity(pub Runner);

impl<S: Send + Sync> FromRequestParts<S> for RunnerIdentity {
    /// A written response rather than a typed rejection: there is one way to
    /// fail and it is not a case a caller can act on, so a type naming it would
    /// be a vocabulary with one word.
    type Rejection = Response;

    /// Not `async fn`: there is nothing here to await. The trait's method is
    /// return-position `impl Future`, so an implementation that already has the
    /// answer hands back a ready one rather than a state machine.
    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<Principal>()
                .and_then(Principal::runner)
                .cloned()
                .map(Self)
                .ok_or_else(unproven),
        )
    }
}

/// The refusal for a handler that ran without its guard.
fn unproven() -> Response {
    let request_id = RequestId::mint();
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let request_id_field = request_id.as_str();
    // `error`, and it is the right level: this is a routing table and a router
    // that disagree, which no amount of client behaviour can cause and no
    // retry can fix.
    tracing::error!(
        error_code = code,
        request_id = request_id_field,
        event = "runner_identity_unproven",
        "a runner handler ran with no proven runner — its guard layer is not mounted"
    );
    ProblemResponse::new(
        error_code::INTERNAL_OPERATION_FAILED,
        DETAIL_UNPROVEN,
        request_id,
    )
    .into_response()
}
