//! Why a backend did not do what a tool asked, in the words the model reads.

use crate::tool::{ErrorCode, ToolOutput};

/// A backend's refusal: the code and the sentence after it.
///
/// A scheduler, a channel and a notifier each answer with one, so the tool
/// over it hands the model the backend's own reason, such as a service's
/// registry code and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The code, such as [`crate::tools::code::INVALID_ARGUMENTS`].
    pub code: ErrorCode,
    /// The sentence after it.
    pub detail: String,
}

impl Refusal {
    /// A refusal under `code`, reading `detail`.
    #[must_use]
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl From<Refusal> for ToolOutput {
    fn from(refusal: Refusal) -> Self {
        Self::failed(refusal.code, &refusal.detail)
    }
}
