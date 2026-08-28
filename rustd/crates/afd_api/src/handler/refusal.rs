//! The refusal a handler carries out through `?`.
//!
//! Its own file rather than a block in the module root, because the two answer
//! different questions: the root is where a request's raw parts are read, and
//! this is the one shape every failure on this surface leaves as. Every
//! constructor below renders a problem document; what varies is which fact the
//! call site alone knows — the state a conflict names, the tag a precondition
//! hands back, the headers a shed carries.

use afd_core::error_code;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, header};

use crate::admission::RETRY_AFTER_SECONDS;
use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The sentence a stream refused at the ceiling carries.
///
/// `error_registry.zig`'s `MSG_SSE_STREAM_CAP`, and it names the INSTANCE on
/// purpose: the ceiling is per process, so a client behind a load balancer may
/// well get a slot on its next attempt.
const DETAIL_STREAM_CEILING: &str = "Concurrent event-stream limit reached on this instance";

use super::malformed;
use super::refusable::{Refusable, log_refusal, refuse};

/// A refusal already rendered, so `?` can carry one out of a handler.
///
/// # Why this exists rather than a `match` at each step
///
/// A handler used to read
///
/// ```ignore
/// let user = match services.users().resolve(subject).await {
///     Ok(user) => user,
///     Err(error) => return refuse(&error, EVENT),
/// };
/// ```
///
/// which is `?` written out by hand, three lines at a time, once per fallible
/// step. The cost is not the typing: a step added later can simply forget its
/// refusal arm and still compile, because nothing forces one. With a handler
/// answering `Result<Response, Refusal>`, a fallible step that forgets to say
/// what its failure means does not compile at all.
///
/// It holds a rendered `Response` rather than an error, because the two planes
/// raise different error types and the thing they have in common is what they
/// render to. The box is `clippy::result_large_err`: a `Response` is over a
/// hundred bytes, and unboxed it would make every handler's `Result` that size
/// for a value the success path never carries.
/// `Debug` so a unit test may `unwrap` a `Result<_, Refusal>`.
///
/// The parsers behind these handlers answer `Result<T, Refusal>`, and a test
/// that asserts on the success arm has to be able to panic with the failure
/// arm rendered. Derived rather than hand-written: what it prints is the
/// response, which is what a failing test needs to read.
#[derive(Debug)]
pub(crate) struct Refusal(Box<Response>);

impl Refusal {
    /// Renders `error` as the refusal for `event`.
    ///
    /// Curried so it reads as `.map_err(Refusal::at(EVENT))?` — the event is
    /// what the call site knows and the error is what the plane hands back.
    pub(crate) fn at<E: Refusable>(event: &'static str) -> impl FnOnce(E) -> Self {
        move |error| Self(Box::new(refuse(&error, event)))
    }

    /// A refusal this daemon wrote itself, with no plane behind it.
    ///
    /// The malformed-input path: a body that will not parse or a path segment
    /// that is not an identifier never reached a plane, so there is no error to
    /// render — only a sentence.
    pub(crate) fn malformed(detail: &'static str) -> Self {
        Self(Box::new(malformed(detail)))
    }

    /// A self-written refusal whose registry code the call site names.
    ///
    /// [`Refusal::malformed`] with the code as a parameter: the library
    /// family's input refusals answer `UZ-LIBRARY-*` codes rather than
    /// `UZ-REQ-001`, because a dashboard tells "bad cursor" from "bad limit"
    /// by the code and has since those endpoints shipped.
    pub(crate) fn coded(code: afd_core::error_code::ErrorCode, detail: &'static str) -> Self {
        Self(Box::new(
            ProblemResponse::new(code, detail, RequestId::mint()).into_response(),
        ))
    }

    /// A conflict this daemon wrote itself, naming the state that refuses.
    ///
    /// [`Refusal::coded`] with `current_state`, for the refusals raised BEFORE
    /// a plane is asked: the steer's ingress check reads a status and decides,
    /// so there is no error to render — only a code, a sentence, and the state
    /// a client branches on without re-reading the resource.
    pub(crate) fn conflict(
        code: afd_core::error_code::ErrorCode,
        detail: &'static str,
        current_state: &str,
    ) -> Self {
        Self(Box::new(
            ProblemResponse::conflict(code, detail, RequestId::mint(), current_state)
                .into_response(),
        ))
    }

    /// The refusal an instance already carrying its ceiling of streams answers.
    ///
    /// A 503 where the request shed is a 429, and a different registry code,
    /// because the remedy is different: a client here reopens the STREAM later,
    /// and throttling its other requests would not free a slot. The status is
    /// the published one — `public/openapi/paths/fleets.yaml` documents 503
    /// `UZ-API-002` with `Retry-After` — and it comes from the code's registry
    /// entry rather than from this call site, which is what keeps the two from
    /// disagreeing.
    ///
    /// `Retry-After` alone, and no `X-RateLimit-*`: those describe a request
    /// quota that resets, and this ceiling is freed by somebody closing a tab.
    /// The brief value is the daemon this ports uses for both ceilings —
    /// optimistic here, but it is what a client is already written against and
    /// a longer one would only delay a retry that costs one round trip.
    pub(crate) fn at_stream_ceiling(carrying: usize, capacity: usize) -> Self {
        let request_id = RequestId::mint();
        let code = error_code::SSE_STREAM_CAP.as_str();
        let id = request_id.as_str();
        tracing::warn!(
            error_code = code,
            request_id = id,
            carrying,
            capacity,
            event = "stream_cap_rejected",
        );
        Self(Box::new(
            (
                [(header::RETRY_AFTER, HeaderValue::from(RETRY_AFTER_SECONDS))],
                ProblemResponse::new(
                    error_code::SSE_STREAM_CAP,
                    DETAIL_STREAM_CEILING,
                    request_id,
                ),
            )
                .into_response(),
        ))
    }

    /// A refusal for a caller who proved who they are and still may not.
    ///
    /// A 403 rather than a 401: the credential is good, and re-presenting it
    /// cannot produce the thing it lacks.
    pub(crate) fn forbidden(detail: &'static str) -> Self {
        Self(Box::new(
            ProblemResponse::new(
                afd_core::error_code::AUTH_FORBIDDEN,
                detail,
                RequestId::mint(),
            )
            .into_response(),
        ))
    }

    /// A refusal for a credential whose context has gone stale under it.
    ///
    /// A 401 where [`Refusal::forbidden`] is a 403, and the difference is the
    /// remedy: here re-authenticating IS the fix — the session names a tenant
    /// that no longer resolves — where a 403's caller would only get the same
    /// answer again.
    pub(crate) fn unauthorized(detail: &'static str) -> Self {
        Self(Box::new(
            ProblemResponse::new(
                afd_core::error_code::AUTH_UNAUTHORIZED,
                detail,
                RequestId::mint(),
            )
            .into_response(),
        ))
    }

    /// A 412 naming the version the resource holds NOW.
    ///
    /// The conditional-write sibling of [`Refusal::conflict_at`], and the
    /// difference is what a client does with it: a 409's `current_state` says
    /// stop retrying, where this hands back the tag an editor needs to
    /// re-apply — so the remedy is one round trip instead of a re-read.
    pub(crate) fn preconditioned(
        code: afd_core::error_code::ErrorCode,
        detail: &'static str,
        etag: &str,
    ) -> Self {
        Self(Box::new(
            ProblemResponse::precondition_failed(code, detail, RequestId::mint(), etag)
                .into_response(),
        ))
    }

    /// Renders `error` as `event`'s refusal, naming the state a 409 carries.
    ///
    /// Curried like [`Refusal::at`], and used INSTEAD of it by the one arm
    /// that knows its refusal is a conflict — the envelope's `current_state`
    /// is what tells a client "stop retrying, the state refuses you", and
    /// only the call site knows which state that is.
    pub(crate) fn conflict_at<E: Refusable>(
        event: &'static str,
        current_state: &'static str,
    ) -> impl FnOnce(E) -> Self {
        move |error| {
            let request_id = RequestId::mint();
            log_refusal(&error, event, &request_id);
            Self(Box::new(
                ProblemResponse::conflict(error.code(), error.detail(), request_id, current_state)
                    .into_response(),
            ))
        }
    }
    /// A conflict whose sentence the CALL SITE composes.
    ///
    /// [`Refusal::conflict_at`] with the detail as a parameter, for the one
    /// refusal whose useful wording carries a number: an operator told their
    /// delete was refused wants to know how many registry entries to go and
    /// remove. `Refusable::detail` is a `&'static str` by the trait's own
    /// contract — one sentence per kind, decided beside its code — so a counted
    /// sentence cannot come from there and is composed here instead. The same
    /// split [`Refusal::preconditioned`] makes for a stale tag.
    ///
    /// Logs through the same path as every other refusal, so the operator's
    /// line is unchanged and only the caller's sentence differs.
    pub(crate) fn conflict_detailed<E: Refusable>(
        event: &'static str,
        detail: String,
        current_state: &'static str,
    ) -> impl FnOnce(E) -> Self {
        move |error| {
            let request_id = RequestId::mint();
            log_refusal(&error, event, &request_id);
            Self(Box::new(
                ProblemResponse::conflict(error.code(), detail, request_id, current_state)
                    .into_response(),
            ))
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        *self.0
    }
}
