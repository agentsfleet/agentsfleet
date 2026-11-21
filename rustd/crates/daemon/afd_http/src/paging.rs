//! The deployment's page sizes: `[api]` in the daemon's file.
//!
//! Every list a handler serves reads its `?limit` bound from here, through
//! [`crate::services::Shared::pages`], rather than from a constant, and the
//! sentence a caller is refused with names the bound it missed.

use af_validate::Ceiling;

/// The page sizes this deployment serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pages {
    /// Every keyset list: the most rows one page holds, and its default.
    pub list: Ceiling,
    /// A fleet's chat: the most messages one page holds, and its default.
    pub thread: Ceiling,
}

impl Default for Pages {
    fn default() -> Self {
        Self {
            list: af_core::paging::CEILING,
            thread: THREAD_CEILING,
        }
    }
}

/// A fleet's chat page when the deployment names none.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the thread page constants are small positive numbers"
)]
const THREAD_CEILING: Ceiling = Ceiling::new(
    afd_turns::view::THREAD_MAX_LIMIT as u32,
    afd_turns::view::THREAD_DEFAULT_LIMIT as u32,
);

/// `"{subject} between 1 and {max}"`: the sentence a page size outside
/// `ceiling` is refused with.
#[must_use]
pub fn limit_sentence(subject: &str, ceiling: Ceiling) -> String {
    format!("{subject} between 1 and {}", ceiling.max())
}

/// The refusal a list's paging earns: the limit's sentence names the
/// configured `ceiling`, and the sort and cursor sentences are the shared ones.
#[must_use]
pub fn refused(
    refusal: af_core::paging::PagingRefusal,
    ceiling: Ceiling,
) -> crate::handler::Refusal {
    match refusal {
        af_core::paging::PagingRefusal::Limit => {
            crate::handler::Refusal::malformed_sentence(limit_sentence("limit must be", ceiling))
        }
        other => crate::handler::Refusal::malformed(other.detail()),
    }
}
