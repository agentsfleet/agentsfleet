//! The operator surface's reads: one statement per view and page position.
//!
//! Copied from `http/handlers/memory/sql.zig`: fleet-scoped, bounded, and
//! keyset-paged over `(created_at, key)` — `created_at` because an upsert moves
//! `updated_at` mid-walk. Each read gains one predicate over the Zig: a fleet
//! granted to read shared memory also sees other fleets' shared rows, through
//! `$2`, which is the workspace for a granted reader and NULL otherwise — and a
//! NULL compares true to nothing.
//!
//! Six statements rather than one built at run time, and one bind order for
//! all six — fleet, shared workspace, filter where there is one, boundary pair
//! where there is one, limit — so one pipeline serves every shape.

use crate::page::View;

/// The LIKE metacharacters a searched-for literal has to be escaped past.
const LIKE_METACHARACTERS: [char; 3] = ['%', '_', '\\'];

/// The escape character the searching statements declare.
const LIKE_ESCAPE: char = '\\';

/// Free-text search, first page. `$1` fleet, `$2` shared workspace,
/// `$3` escaped pattern, `$4` limit.
const SEARCH_ENTRIES: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND (key ILIKE $3 ESCAPE '\\' OR content ILIKE $3 ESCAPE '\\')
ORDER BY created_at DESC, key DESC
LIMIT $4";

/// [`SEARCH_ENTRIES`] past a boundary. `$4` instant, `$5` key, `$6` limit.
const SEARCH_ENTRIES_AFTER: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND (key ILIKE $3 ESCAPE '\\' OR content ILIKE $3 ESCAPE '\\')
  AND (created_at, key) < ($4, $5)
ORDER BY created_at DESC, key DESC
LIMIT $6";

/// One category, first page. `$3` category, `$4` limit.
const SELECT_ENTRIES_IN_CATEGORY: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND category = $3
ORDER BY created_at DESC, key DESC LIMIT $4";

/// [`SELECT_ENTRIES_IN_CATEGORY`] past a boundary. `$4` instant, `$5` key,
/// `$6` limit.
const SELECT_ENTRIES_IN_CATEGORY_AFTER: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND category = $3
  AND (created_at, key) < ($4, $5)
ORDER BY created_at DESC, key DESC LIMIT $6";

/// Everything, first page. `$3` limit.
const SELECT_RECENT_ENTRIES: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
ORDER BY created_at DESC, key DESC LIMIT $3";

/// [`SELECT_RECENT_ENTRIES`] past a boundary. `$3` instant, `$4` key,
/// `$5` limit.
const SELECT_RECENT_ENTRIES_AFTER: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE (fleet_id = $1::uuid OR (workspace_id = $2::uuid AND workspace_visible))
  AND (created_at, key) < ($3, $4)
ORDER BY created_at DESC, key DESC LIMIT $5";

/// The statement `view` runs, on a first page or a continuation.
pub(super) const fn statement(view: View<'_>, resuming: bool) -> &'static str {
    match (view, resuming) {
        (View::Recent, false) => SELECT_RECENT_ENTRIES,
        (View::Recent, true) => SELECT_RECENT_ENTRIES_AFTER,
        (View::Category(_label), false) => SELECT_ENTRIES_IN_CATEGORY,
        (View::Category(_label), true) => SELECT_ENTRIES_IN_CATEGORY_AFTER,
        (View::Search(_text), false) => SEARCH_ENTRIES,
        (View::Search(_text), true) => SEARCH_ENTRIES_AFTER,
    }
}

/// The value `view` binds after the shared workspace, where it has one.
///
/// Owned because the search arm BUILDS its value: the escaped pattern is not a
/// substring of anything the caller sent.
pub(super) fn filter(view: View<'_>) -> Option<String> {
    match view {
        View::Recent => None,
        View::Category(label) => Some(label.to_owned()),
        View::Search(text) => Some(pattern(text)),
    }
}

/// `text` as a contains-pattern with its LIKE metacharacters escaped.
pub(super) fn pattern(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('%');
    for character in text.chars() {
        if LIKE_METACHARACTERS.contains(&character) {
            escaped.push(LIKE_ESCAPE);
        }
        escaped.push(character);
    }
    escaped.push('%');
    escaped
}

#[cfg(test)]
mod tests {
    use super::{filter, pattern, statement};
    use crate::page::View;

    #[test]
    fn should_escape_every_like_metacharacter() {
        assert_eq!(pattern("hello"), "%hello%");
        assert_eq!(pattern(""), "%%");
        assert_eq!(pattern("100%"), "%100\\%%");
        assert_eq!(pattern("a_b"), "%a\\_b%");
        assert_eq!(pattern("a\\b"), "%a\\\\b%");
    }

    /// A category is bound as itself: it is compared with `=`, not `LIKE`.
    #[test]
    fn should_bind_a_category_verbatim() {
        assert_eq!(filter(View::Category("100%")).as_deref(), Some("100%"));
        assert_eq!(filter(View::Recent), None);
        assert_eq!(
            filter(View::Search("mon%day")).as_deref(),
            Some("%mon\\%day%")
        );
    }

    /// Every view answers a distinct statement per page position, and only a
    /// continuation seeks past the boundary.
    #[test]
    fn should_choose_one_statement_per_view_and_position() {
        const SEEK: &str = "(created_at, key) <";
        let views = [View::Recent, View::Category("core"), View::Search("x")];
        let mut seen: Vec<_> = views
            .iter()
            .flat_map(|view| [statement(*view, false), statement(*view, true)])
            .collect();
        for view in views {
            assert!(!statement(view, false).contains(SEEK));
            assert!(statement(view, true).contains(SEEK));
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 6, "no two views share a statement");
    }
}
