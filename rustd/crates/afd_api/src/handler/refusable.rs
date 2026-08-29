//! What a plane's failure tells the HTTP edge, and the one line every refusal
//! leaves behind.
//!
//! Split from [`super`] along the seam the two halves already draw: this file
//! is about ERRORS — what each domain crate can be asked about its own — while
//! the module beside it is about RESPONSES. Every crate that grows a fallible
//! surface adds an impl here and touches nothing else, which is why the list
//! below is long and the file it used to share is no longer over its cap.

use axum::response::{IntoResponse as _, Response};

use afd_core::error_code::ErrorCode;

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

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

impl Refusable for afd_billing::Error {
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

impl Refusable for afd_cron::Error {
    fn code(&self) -> ErrorCode {
        Self::code(self)
    }
    fn detail(&self) -> &'static str {
        Self::detail(self)
    }
    fn is_datastore_unavailable(&self) -> bool {
        Self::code(self) == afd_core::error_code::INTERNAL_DB_UNAVAILABLE
    }
    fn reason(&self) -> String {
        self.to_string()
    }
}

impl Refusable for afd_approval::Error {
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

impl Refusable for afd_runner::Error {
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

impl Refusable for afd_vault::Error {
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

impl Refusable for afd_events::Error {
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

impl Refusable for afd_fleet_ops::Error {
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

impl Refusable for afd_admin::Error {
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

impl Refusable for afd_ingress::Error {
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

impl Refusable for afd_library::Error {
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

/// Turns a control-plane failure into the response its code prescribes.
///
/// The status is NOT chosen here. `afd_core::problem` holds one status per
/// code, so a datastore that would not answer is a 503 wherever it is raised
/// and a rejected field is a 400 wherever it is raised — which is what keeps
/// the runner client's backoff working, since it classifies on the transport
/// class rather than on the verb (RULE ECL).
pub(crate) fn refuse<E: Refusable + ?Sized>(error: &E, event: &'static str) -> Response {
    let request_id = RequestId::mint();
    log_refusal(error, event, &request_id);
    ProblemResponse::new(error.code(), error.detail(), request_id).into_response()
}

/// The log line every rendered refusal leaves, whatever its envelope.
pub(super) fn log_refusal<E: Refusable + ?Sized>(
    error: &E,
    event: &'static str,
    request_id: &RequestId,
) {
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
