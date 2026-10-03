//! The shape of one operator page: which rows, and where it resumes.
//!
//! Store-agnostic values. How a store turns a view into a statement is the
//! store's business — see the Postgres store's `page` module.

use crate::error::detail::{LIST_FAILED, SEARCH_FAILED};

/// Which rows one page reads.
///
/// An enum rather than two optional parameters: a page has exactly ONE view,
/// so the precedence `handler.zig` resolves with an if-ladder — search beats
/// category beats recent — has nothing left to get wrong here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View<'a> {
    /// Everything the fleet remembers, newest first.
    Recent,
    /// One retention category of it.
    Category(&'a str),
    /// Entries whose key or content contains this text.
    Search(&'a str),
}

impl View<'_> {
    /// The sentence a refused statement on this view answers with.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Search(_text) => SEARCH_FAILED,
            Self::Recent | Self::Category(..) => LIST_FAILED,
        }
    }

    /// Whether an empty page from this view is a failed RECALL.
    ///
    /// Only a search is. A list or a category coming back empty means the
    /// fleet has learned nothing yet, or nothing under that label.
    #[must_use]
    pub const fn is_recall(self) -> bool {
        matches!(self, Self::Search(_text))
    }
}

/// Where a page resumes: the boundary row's `(created_at, key)`.
///
/// `created_at` and not `updated_at`: an upsert moves a row's `updated_at`
/// mid-walk, and a cursor over a column that moves under it skips or repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct After<'a> {
    /// The boundary row's creation instant.
    pub created_at_ms: i64,
    /// Its key, which breaks a tie inside one millisecond.
    pub key: &'a str,
}

#[cfg(test)]
mod tests {
    use super::View;

    /// Only a search's empty page is evidence of a failed recall.
    #[test]
    fn should_count_only_a_search_as_recall() {
        assert!(View::Search("x").is_recall());
        assert!(!View::Recent.is_recall());
        assert!(!View::Category("core").is_recall());
    }

    /// A search and a list carry different sentences under one code.
    #[test]
    fn should_name_the_operation_a_refused_statement_came_from() {
        assert_eq!(View::Search("x").detail(), "memory search failed");
        assert_eq!(View::Recent.detail(), "memory list failed");
        assert_eq!(View::Category("core").detail(), "memory list failed");
    }
}
