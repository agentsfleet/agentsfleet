//! Private setup errors retain their current state and strong version.

use super::{Refusable, Refusal};
use afd_authoring::Error;
use afd_core::error_code::ErrorCode;

const EVENT: &str = "authoring_request_refused";

impl Refusable for Error {
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
        // Validation causes may name authored keys. Private input stays out
        // of logs; the error itself still retains its original source chain.
        format!("[{}] {}", self.code(), self.detail())
    }
}

/// Renders the safe domain refusal, retaining recovery context when available.
#[must_use]
pub fn authoring_refusal(error: Error) -> Refusal {
    if let Some(digest) = error.review_digest() {
        Refusal::review_changed(error.detail(), digest)
    } else if let Some(etag) = error.etag() {
        Refusal::preconditioned(error.code(), error.detail(), etag)
    } else if let Some(state) = error.current_state() {
        Refusal::conflict_at(EVENT, state)(error)
    } else {
        Refusal::at(EVENT)(error)
    }
}
