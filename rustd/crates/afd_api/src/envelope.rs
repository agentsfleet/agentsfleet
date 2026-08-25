//! The `application/problem+json` refusal, and the only way to write one.
//!
//! RFC 7807. The status, title, documentation link and end-user sentence all
//! come from [`afd_core::problem`] rather than from the call site, so a code
//! answers the same status everywhere it is raised. A caller supplies only what
//! it alone knows: the detail, and the request id.
//!
//! # What is deliberately NOT on the wire
//!
//! `Problem::hint` is operator-facing — it tells whoever runs this what to go
//! and look at — and the Zig envelope has never serialised it. Putting it in a
//! client response would leak internal remediation advice to whoever can
//! provoke an error, so the omission is a decision and
//! `test_hint_never_reaches_the_client` is what keeps it one.
//!
//! # Extensions ride the base envelope, they do not replace it
//!
//! Two status-specific fields exist (section 3.2 of that RFC), and each appears only on
//! the status that mandates it: `current_state` on a 409, naming the state that
//! forbade the transition, and `etag` on a 412, so a client can refetch and
//! rebase rather than guess what it raced with. Both are omitted from the wire
//! otherwise — absent, not null — so every other response's shape is untouched.

use afd_core::error_code::ErrorCode;
use afd_core::problem::Problem;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use serde_json::{Map, Value};

/// The media type every refusal carries.
pub const CONTENT_TYPE_PROBLEM_JSON: &str = "application/problem+json";

/// A refusal, ready to become a response.
#[derive(Debug, Clone)]
pub struct ProblemResponse {
    problem: Problem,
    detail: String,
    request_id: String,
    current_state: Option<String>,
    etag: Option<String>,
}

impl ProblemResponse {
    /// A refusal for `code`, with the detail and request id only the caller
    /// knows.
    ///
    /// The status is NOT a parameter. It is a property of the code, and two
    /// call sites answering different statuses for one code is the bug this
    /// shape prevents.
    #[must_use]
    pub fn new(code: ErrorCode, detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            problem: Problem::of(code),
            detail: detail.into(),
            request_id: request_id.into(),
            current_state: None,
            etag: None,
        }
    }

    /// A 409, carrying the state that forbade the transition.
    ///
    /// A conflict that does not say what it conflicted WITH leaves the caller
    /// to guess, and the guess is usually a retry loop against a state that
    /// will refuse it every time.
    #[must_use]
    pub fn conflict(
        code: ErrorCode,
        detail: impl Into<String>,
        request_id: impl Into<String>,
        current_state: impl Into<String>,
    ) -> Self {
        Self {
            current_state: Some(current_state.into()),
            ..Self::new(code, detail, request_id)
        }
    }

    /// A 412, carrying the resource's current version.
    #[must_use]
    pub fn precondition_failed(
        code: ErrorCode,
        detail: impl Into<String>,
        request_id: impl Into<String>,
        etag: impl Into<String>,
    ) -> Self {
        Self {
            etag: Some(etag.into()),
            ..Self::new(code, detail, request_id)
        }
    }

    /// The status this refusal answers with.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        // A registry entry carries a status that is a real one; the fallback
        // exists so writing a response is total, and cannot be reached by a
        // code this workspace declares.
        StatusCode::from_u16(self.problem.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// The wire body, in the field order the Zig envelope writes.
    ///
    /// Built as a map rather than a `#[derive(Serialize)]` struct, for two
    /// reasons that happen to point the same way. `serde_derive` is a syn-2
    /// proc macro and the rest of this graph is on syn 3 — the duplicate fails
    /// `clippy::multiple_crate_versions`, which is the same trade the workspace
    /// manifest already records for `zeroize_derive`. And `serde_json` carries
    /// `preserve_order`, so insertion order IS wire order: the field sequence
    /// is stated here, in one place, rather than inferred from a struct
    /// definition somewhere else.
    fn body(&self) -> Value {
        let mut body = Map::new();
        body.insert("docs_uri".to_owned(), self.problem.docs_uri().into());
        body.insert("title".to_owned(), self.problem.title().into());
        body.insert("detail".to_owned(), self.detail.clone().into());
        body.insert("error_code".to_owned(), self.problem.code().as_str().into());
        body.insert("request_id".to_owned(), self.request_id.clone().into());
        // Absent extensions are ABSENT, never null. RFC 7807 defines them per
        // status, so a client is invited to switch on key presence; emitting
        // `"etag": null` on every response makes that switch wrong everywhere.
        if let Some(current_state) = &self.current_state {
            body.insert("current_state".to_owned(), current_state.clone().into());
        }
        if let Some(etag) = &self.etag {
            body.insert("etag".to_owned(), etag.clone().into());
        }
        if let Some(user_message) = self.problem.user_message() {
            body.insert("user_message".to_owned(), user_message.into());
        }
        Value::Object(body)
    }
}

impl IntoResponse for ProblemResponse {
    fn into_response(self) -> Response {
        // `Value::to_string` cannot fail — it writes to a String — so there is
        // no serialisation-failure arm here. The Zig writer has one because
        // `std.json.fmt` writes to a fixed response buffer that can run out;
        // reproducing it in Rust would be a branch nothing can reach, which is
        // dead code wearing a safety jacket.
        (
            self.status(),
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(CONTENT_TYPE_PROBLEM_JSON),
            )],
            self.body().to_string(),
        )
            .into_response()
    }
}
