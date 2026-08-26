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

pub mod runner;

use axum::response::{IntoResponse as _, Response};

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

pub(crate) fn refuse(error: &afd_fleet::Error, event: &'static str) -> Response {
    let request_id = RequestId::mint();
    let code = error.code();
    // Hoisted out of the macro: `tracing`'s `log` bridge compiles a second copy
    // of every field expression, and llvm-cov scores the copy that never runs.
    let code_field = code.as_str();
    let request_id_field = request_id.as_str();
    // The whole chain, which is where the cause a client is NOT told lives.
    // `Display` on this error renders its code and its kind; the `source()`
    // beneath it is what names the statement or the pool.
    let reason = error.to_string();
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
    ProblemResponse::new(code, error.detail(), request_id).into_response()
}
