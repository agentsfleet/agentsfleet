//! The verbs this binary answers, and the one way they refuse.
//!
//! A handler here does three things and no others: read what the request
//! carries, call one service method, and turn the answer into a response. It
//! decides nothing — the identity was proven by the layer in front of it, the
//! validation lives in the service, and the status a refusal answers with is a
//! property of the error's code rather than of the call site.
//!
//! # Why the refusal writer is shared
//!
//! `service.zig`'s handlers each spell their own `hx.fail(code, detail)` pairs,
//! twelve times over, and nothing relates a code to its sentence — so two
//! handlers can describe one failure differently and both compile. Here
//! [`refuse`] takes the ERROR, and the error already knows both
//! (`afd_fleet::Error::code` and `::detail`). There is no pair to get wrong
//! because there is no pair to write.

pub mod auth;
pub mod fleet;
pub mod runner;
pub mod tenant;

use axum::response::{IntoResponse, Response};

use afd_core::error_code::ErrorCode;

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// Turns a control-plane failure into the response its code prescribes.
///
/// The status is NOT chosen here. `afd_core::problem` holds one status per
/// code, so a datastore that would not answer is a 503 wherever it is raised
/// and a rejected field is a 400 wherever it is raised — which is what keeps
/// the runner client's backoff working, since it classifies on the transport
/// class rather than on the verb (RULE ECL).
/// Refuses a request this daemon cannot read at all.
///
/// A path segment that is not an identifier, or a body that is not the shape
/// the verb takes, can never reach a row — so it is refused BEFORE the plane is
/// asked. That keeps the `::uuid` cast in the statements from ever being the
/// thing that fails, and leaves every error from below a genuine datastore
/// fault.
///
/// Shared by every verb that reads one, rather than restated per handler: two
/// spellings would be two different envelopes for one class of refusal.
pub(crate) fn malformed(detail: &'static str) -> Response {
    crate::envelope::ProblemResponse::new(
        afd_core::error_code::INVALID_REQUEST,
        detail,
        crate::request_id::RequestId::mint(),
    )
    .into_response()
}

/// One query-string parameter, by name.
///
/// A hand-rolled scan rather than a query-string crate, because that is the
/// whole of what these handlers need from a query string and a crate for it
/// would be a dependency to justify. Percent-decoding is deliberately absent:
/// every value these parameters take — a limit, a sort spelling, a cursor —
/// is drawn from an alphabet that survives a URL unescaped, and a decoder here
/// would be a second place for a `+` to become a space.
pub(crate) fn parameter<'q>(query: &'q str, name: &str) -> Option<&'q str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// What a domain crate's error can tell the HTTP edge about itself.
///
/// Three questions, and every plane answers all three: which registry code,
/// which sentence, and whether the datastore was the thing that failed. A trait
/// rather than one concrete type because there is now more than one plane —
/// `afd_fleet` answers for the runner, `afd_tenant` for the tenant — and each
/// owns its own error per `RUST_ERROR_STANDARD`. The edge does not need to know
/// which it is holding; it needs these three answers.
pub(crate) trait Refusable {
    /// The registry code this failure answers with.
    fn code(&self) -> ErrorCode;
    /// The sentence the caller is told.
    fn detail(&self) -> &'static str;
    /// Whether the datastore behind the plane could not be reached.
    fn is_datastore_unavailable(&self) -> bool;
    /// The whole causal chain, for the log and never for the caller.
    fn reason(&self) -> String;
}

impl Refusable for afd_fleet::Error {
    fn code(&self) -> ErrorCode {
        Self::code(self)
    }
    fn detail(&self) -> &'static str {
        Self::detail(self)
    }
    fn is_datastore_unavailable(&self) -> bool {
        Self::is_datastore_unavailable(self)
    }
    fn reason(&self) -> String {
        self.to_string()
    }
}

impl Refusable for afd_fleet_lifecycle::Error {
    fn code(&self) -> ErrorCode {
        Self::code(self)
    }
    fn detail(&self) -> &'static str {
        Self::detail(self)
    }
    fn is_datastore_unavailable(&self) -> bool {
        Self::is_datastore_unavailable(self)
    }
    fn reason(&self) -> String {
        self.to_string()
    }
}

impl Refusable for afd_tenant::Error {
    fn code(&self) -> ErrorCode {
        Self::code(self)
    }
    fn detail(&self) -> &'static str {
        Self::detail(self)
    }
    fn is_datastore_unavailable(&self) -> bool {
        Self::is_datastore_unavailable(self)
    }
    fn reason(&self) -> String {
        self.to_string()
    }
}

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
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        *self.0
    }
}

pub(crate) fn refuse<E: Refusable + ?Sized>(error: &E, event: &'static str) -> Response {
    let request_id = RequestId::mint();
    log_refusal(error, event, &request_id);
    ProblemResponse::new(error.code(), error.detail(), request_id).into_response()
}

/// The log line every rendered refusal leaves, whatever its envelope.
fn log_refusal<E: Refusable + ?Sized>(error: &E, event: &'static str, request_id: &RequestId) {
    // Hoisted out of the macro: `tracing`'s `log` bridge compiles a second copy
    // of every field expression, and llvm-cov scores the copy that never runs.
    let code_field = error.code().as_str();
    let request_id_field = request_id.as_str();
    // The whole chain, which is where the cause a client is NOT told lives.
    // `Display` on this error renders its code and its kind; the `source()`
    // beneath it is what names the statement or the pool.
    let reason = error.reason();
    if error.is_datastore_unavailable() {
        // An outage is the instance's problem to report, and the caller's to
        // back off from. `warn` rather than `error`: the incident belongs to
        // whichever datastore is down, and paging twice for it helps nobody.
        tracing::warn!(
            error_code = code_field,
            request_id = request_id_field,
            reason,
            event,
        );
    } else {
        // Everything else is either the caller's request or one statement, and
        // neither is worth waking anybody for on its own.
        tracing::debug!(
            error_code = code_field,
            request_id = request_id_field,
            reason,
            event,
        );
    }
}
