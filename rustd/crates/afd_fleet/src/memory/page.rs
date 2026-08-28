//! The shape of one page: which rows, where it resumes, and what a row holds.
//!
//! Split from [`super::operator`], which owns the two verbs that RUN these — a
//! value here knows nothing about a pool, so the whole of the operator
//! surface's decision-making is proven by the unit tests at the bottom of this
//! file without a datastore anywhere near them.

use super::sql;
use crate::error::{DETAIL_MEMORY_LIST_FAILED, DETAIL_MEMORY_SEARCH_FAILED};

/// The LIKE metacharacters a searched-for literal has to be escaped past.
const LIKE_METACHARACTERS: [char; 3] = ['%', '_', '\\'];

/// The escape character the searching statements declare.
const LIKE_ESCAPE: char = '\\';

/// Which rows one page reads.
///
/// An enum rather than two optional parameters, and the difference is what the
/// type can express: `handler.zig` carries `query_text` and `category` as
/// separate optionals and resolves them with an if-ladder, so the precedence —
/// search beats category beats recent — lives only in that ladder and the
/// both-set case is decided three statements away from where it is read. A page
/// has exactly ONE view, so here there is no precedence left to get wrong.
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
    ///
    /// Pinned to `handler.zig`, which writes a different one per view — see
    /// [`crate::error::report::memory_unavailable`] for why one registry code
    /// carries more than one sentence on this surface.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Search(_text) => DETAIL_MEMORY_SEARCH_FAILED,
            Self::Recent | Self::Category(..) => DETAIL_MEMORY_LIST_FAILED,
        }
    }

    /// Whether an empty page from this view is a failed RECALL.
    ///
    /// Only a search is. A list or a category filter coming back empty means
    /// the fleet has learned nothing yet, or nothing under that label — an
    /// ordinary answer, not evidence that memory failed to surface something it
    /// holds.
    #[must_use]
    pub const fn is_recall(self) -> bool {
        matches!(self, Self::Search(_text))
    }

    /// The value bound as `$2`, where the view has one.
    ///
    /// Owned rather than borrowed because the search arm BUILDS its value: the
    /// escaped pattern is not a substring of anything the caller sent.
    #[must_use]
    pub fn filter(self) -> Option<String> {
        match self {
            Self::Recent => None,
            Self::Category(label) => Some(label.to_owned()),
            Self::Search(text) => Some(format!("%{}%", escape_like(text))),
        }
    }

    /// The statement this view runs, on a first page or a continuation.
    #[must_use]
    pub const fn statement(self, resuming: bool) -> &'static str {
        match (self, resuming) {
            (Self::Recent, false) => sql::SELECT_RECENT_ENTRIES,
            (Self::Recent, true) => sql::SELECT_RECENT_ENTRIES_AFTER,
            (Self::Category(_label), false) => sql::SELECT_ENTRIES_IN_CATEGORY,
            (Self::Category(_label), true) => sql::SELECT_ENTRIES_IN_CATEGORY_AFTER,
            (Self::Search(_text), false) => sql::SEARCH_ENTRIES,
            (Self::Search(_text), true) => sql::SEARCH_ENTRIES_AFTER,
        }
    }
}

/// Where a page resumes: the boundary row's `(created_at, key)`.
///
/// `created_at` and not `updated_at`, which is why the operator reads order
/// differently from hydration: an upsert moves a row's `updated_at` mid-walk,
/// and a cursor over a column that moves under it skips rows or repeats them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct After<'a> {
    /// The boundary row's creation instant.
    pub created_at_ms: i64,
    /// Its key, which breaks a tie inside one millisecond.
    pub key: &'a str,
}

/// One stored entry, as the operator surface reads it.
///
/// Carries `created_at_ms`, which no wire shape does: it is the other half of
/// the keyset boundary the next page resumes from, and reading it here is what
/// stops a caller having to infer it from `updated_at` — the column that moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The stable key the fleet remembers this under.
    pub key: String,
    /// What it remembers.
    pub content: String,
    /// The retention category, which decides eviction order.
    pub category: String,
    /// When it was last written.
    pub updated_at_ms: i64,
    /// When it was first written — the cursor's sort value.
    pub created_at_ms: i64,
}

/// Escapes the LIKE metacharacters in `input`.
///
/// `helpers.zig`'s `escapeLikePattern`, and what it produces is what the
/// searching statements' `ESCAPE '\'` clause reads: a person searching for a
/// literal `%` matches that character rather than every row the fleet holds.
fn escape_like(input: &str) -> String {
    input.chars().fold(
        String::with_capacity(input.len()),
        |mut escaped, character| {
            if LIKE_METACHARACTERS.contains(&character) {
                escaped.push(LIKE_ESCAPE);
            }
            escaped.push(character);
            escaped
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{After, View, escape_like};
    use crate::memory::sql;

    #[test]
    fn should_leave_a_plain_search_unescaped() {
        assert_eq!(escape_like("hello"), "hello");
        assert_eq!(escape_like(""), "");
    }

    #[test]
    fn should_escape_every_like_metacharacter() {
        assert_eq!(escape_like("100%"), "100\\%");
        assert_eq!(escape_like("a_b"), "a\\_b");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        assert_eq!(escape_like("%_\\"), "\\%\\_\\\\");
    }

    /// A wildcard a person typed matches the character, never every row.
    #[test]
    fn should_wrap_a_searched_literal_in_its_own_wildcards() {
        assert_eq!(
            View::Search("mon%day").filter().as_deref(),
            Some("%mon\\%day%")
        );
    }

    /// A category is bound as itself: it is compared with `=`, not `LIKE`.
    #[test]
    fn should_bind_a_category_verbatim() {
        assert_eq!(View::Category("100%").filter().as_deref(), Some("100%"));
        assert_eq!(View::Recent.filter(), None);
    }

    /// Every view answers a distinct statement per page position.
    #[test]
    fn should_choose_one_statement_per_view_and_position() {
        let chosen = [
            View::Recent.statement(false),
            View::Recent.statement(true),
            View::Category("core").statement(false),
            View::Category("core").statement(true),
            View::Search("x").statement(false),
            View::Search("x").statement(true),
        ];
        assert_eq!(chosen[0], sql::SELECT_RECENT_ENTRIES);
        assert_eq!(chosen[1], sql::SELECT_RECENT_ENTRIES_AFTER);
        assert_eq!(chosen[2], sql::SELECT_ENTRIES_IN_CATEGORY);
        assert_eq!(chosen[3], sql::SELECT_ENTRIES_IN_CATEGORY_AFTER);
        assert_eq!(chosen[4], sql::SEARCH_ENTRIES);
        assert_eq!(chosen[5], sql::SEARCH_ENTRIES_AFTER);
        let mut seen = chosen.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), chosen.len(), "no two views share a statement");
    }

    /// A continuation statement seeks past the boundary; a first page has none.
    #[test]
    fn should_seek_past_the_boundary_only_when_resuming() {
        const SEEK: &str = "(created_at, key) <";
        for view in [View::Recent, View::Category("core"), View::Search("x")] {
            assert!(!view.statement(false).contains(SEEK));
            assert!(view.statement(true).contains(SEEK));
        }
    }

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

    /// A boundary is `(created_at, key)`, and copyable so a page can pass it on.
    #[test]
    fn should_carry_the_boundary_as_a_pair() {
        let boundary = After {
            created_at_ms: 1_700_000_000_000,
            key: "goal:current",
        };
        assert_eq!(boundary, boundary);
    }
}
